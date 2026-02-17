//! SQLite database provider.
//!
//! Shells out to the `sqlite3` CLI for query execution, parsing its
//! separator-delimited output into the shared [`DatabaseProvider`] types.

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::debug;

use super::{
    cell_to_json_value, parse_separated_output, run_cli_command, ColumnInfo, DatabaseProvider,
    DatabaseType, ForeignKey, IndexInfo, QueryResult, SchemaInfo, TableDescription, TableInfo,
};

/// SQLite provider backed by the `sqlite3` command-line tool.
pub struct SQLiteProvider {
    db_path: String,
}

impl SQLiteProvider {
    /// Create a new provider for the database file at `db_path`.
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    /// Return the path to the database file.
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    /// Execute a SQL statement via `sqlite3` and return raw stdout.
    ///
    /// Uses `-separator '|'` for easy parsing and `-bail` to stop on error.
    async fn sqlite_query(&self, sql: &str) -> Result<String> {
        run_cli_command(
            "sqlite3",
            &[&self.db_path, "-separator", "|", "-bail", sql],
            &[],
        )
        .await
    }

    /// Execute a SQL statement via `sqlite3` with column headers enabled.
    async fn sqlite_query_with_headers(&self, sql: &str) -> Result<String> {
        run_cli_command(
            "sqlite3",
            &[&self.db_path, "-header", "-separator", "|", "-bail", sql],
            &[],
        )
        .await
    }
}

#[async_trait]
impl DatabaseProvider for SQLiteProvider {
    fn db_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    async fn test_connection(&self) -> Result<()> {
        debug!(path = %self.db_path, "testing SQLite connection");
        self.sqlite_query("SELECT 1")
            .await
            .context("SQLite connection test failed")?;
        Ok(())
    }

    async fn list_schemas(&self) -> Result<Vec<SchemaInfo>> {
        debug!("listing SQLite schemas");

        // SQLite has a single implicit schema called "main".
        let sql = "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'";
        let output = self.sqlite_query(sql).await?;
        let count: usize = output.trim().parse().unwrap_or(0);

        Ok(vec![SchemaInfo {
            name: "main".to_string(),
            table_count: count,
        }])
    }

    async fn list_tables(&self, _schema: &str) -> Result<Vec<TableInfo>> {
        debug!("listing SQLite tables");

        // List all user tables with estimated row counts.
        // SQLite does not expose row counts in metadata, so we query each table.
        let tables_sql =
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
        let output = self.sqlite_query(tables_sql).await?;
        let table_names: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        let mut tables = Vec::new();
        for name in table_names {
            // Get approximate row count. This is a full table scan for each
            // table, which is acceptable for the metadata-gathering use case.
            let count_sql = format!("SELECT COUNT(*) FROM \"{name}\"");
            let count_output = self.sqlite_query(&count_sql).await.unwrap_or_default();
            let row_count: Option<u64> = count_output.trim().parse().ok();

            tables.push(TableInfo {
                name,
                schema: "main".to_string(),
                row_count_estimate: row_count,
                // SQLite does not expose per-table size easily.
                size_bytes: None,
            });
        }

        Ok(tables)
    }

    async fn describe_table(&self, _schema: &str, table: &str) -> Result<TableDescription> {
        debug!(table = %table, "describing SQLite table");

        // ── Columns via PRAGMA ───────────────────────────────────
        let pragma_sql = format!("PRAGMA table_info(\"{table}\")");
        let output = self.sqlite_query(&pragma_sql).await?;
        let rows = parse_separated_output(&output, '|');

        // PRAGMA table_info returns: cid|name|type|notnull|dflt_value|pk
        let mut columns = Vec::new();
        let mut primary_key = Vec::new();

        for cols in &rows {
            if cols.len() >= 6 {
                let is_pk = cols[5] != "0";
                let name = cols[1].clone();
                if is_pk {
                    primary_key.push(name.clone());
                }
                columns.push(ColumnInfo {
                    name,
                    data_type: cols[2].clone(),
                    nullable: cols[3] == "0", // notnull: 0 means nullable
                    default_value: if cols[4].is_empty() {
                        None
                    } else {
                        Some(cols[4].clone())
                    },
                    is_primary_key: is_pk,
                });
            }
        }

        // ── Foreign keys ─────────────────────────────────────────
        let fk_sql = format!("PRAGMA foreign_key_list(\"{table}\")");
        let fk_output = self.sqlite_query(&fk_sql).await?;
        let fk_rows = parse_separated_output(&fk_output, '|');

        // PRAGMA foreign_key_list returns: id|seq|table|from|to|on_update|on_delete|match
        let foreign_keys: Vec<ForeignKey> = fk_rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 5 {
                    Some(ForeignKey {
                        column: cols[3].clone(),
                        references_table: cols[2].clone(),
                        references_column: cols[4].clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        // ── Indexes ──────────────────────────────────────────────
        let idx_sql = format!("PRAGMA index_list(\"{table}\")");
        let idx_output = self.sqlite_query(&idx_sql).await?;
        let idx_rows = parse_separated_output(&idx_output, '|');

        // PRAGMA index_list returns: seq|name|unique|origin|partial
        let mut indexes = Vec::new();
        for cols in &idx_rows {
            if cols.len() >= 3 {
                let idx_name = &cols[1];
                let unique = cols[2] == "1";

                // Get columns for this index.
                let info_sql = format!("PRAGMA index_info(\"{idx_name}\")");
                let info_output = self.sqlite_query(&info_sql).await?;
                let info_rows = parse_separated_output(&info_output, '|');

                // PRAGMA index_info returns: seqno|cid|name
                let idx_columns: Vec<String> = info_rows
                    .into_iter()
                    .filter_map(|c| if c.len() >= 3 { Some(c[2].clone()) } else { None })
                    .collect();

                indexes.push(IndexInfo {
                    name: idx_name.clone(),
                    columns: idx_columns,
                    unique,
                });
            }
        }

        Ok(TableDescription {
            name: table.to_string(),
            schema: "main".to_string(),
            columns,
            primary_key,
            foreign_keys,
            indexes,
        })
    }

    async fn sample_rows(
        &self,
        _schema: &str,
        table: &str,
        limit: u32,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        debug!(table = %table, limit = limit, "sampling SQLite rows");

        let sql = format!("SELECT * FROM \"{table}\" LIMIT {limit}");
        let output = self.sqlite_query_with_headers(&sql).await?;
        let lines: Vec<&str> = output.lines().collect();

        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let headers: Vec<String> = lines[0]
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();

        let mut results = Vec::new();
        for line in &lines[1..] {
            if line.is_empty() {
                continue;
            }
            let values: Vec<&str> = line.split('|').collect();
            let mut row = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                let val = values.get(i).unwrap_or(&"");
                row.insert(header.clone(), cell_to_json_value(val));
            }
            results.push(row);
        }

        Ok(results)
    }

    async fn execute_query(&self, sql: &str) -> Result<QueryResult> {
        debug!(sql = %sql, "executing SQLite query");
        let start = Instant::now();

        let output = self.sqlite_query_with_headers(sql).await?;
        let elapsed = start.elapsed().as_millis() as u64;

        let lines: Vec<&str> = output.lines().collect();

        if lines.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                rows_affected: None,
                execution_time_ms: elapsed,
            });
        }

        let columns: Vec<String> = lines[0]
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();

        let mut rows = Vec::new();
        for line in &lines[1..] {
            if line.is_empty() {
                continue;
            }
            let values: Vec<serde_json::Value> = line
                .split('|')
                .map(|cell| cell_to_json_value(cell))
                .collect();
            rows.push(values);
        }

        let rows_affected = Some(rows.len() as u64);

        Ok(QueryResult {
            columns,
            rows,
            rows_affected,
            execution_time_ms: elapsed,
        })
    }

    async fn get_context_summary(&self) -> Result<String> {
        debug!("building SQLite context summary");

        // Get all user tables.
        let tables_sql =
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name";
        let output = self.sqlite_query(tables_sql).await?;
        let table_names: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        if table_names.is_empty() {
            return Ok(String::from("(No user tables found)"));
        }

        let mut summary = String::new();

        for (i, table) in table_names.iter().enumerate() {
            if i > 0 {
                summary.push('\n');
            }
            summary.push_str(&format!("Table: {table}\n"));

            let pragma_sql = format!("PRAGMA table_info(\"{table}\")");
            let pragma_output = self.sqlite_query(&pragma_sql).await?;
            let rows = parse_separated_output(&pragma_output, '|');

            // PRAGMA table_info: cid|name|type|notnull|dflt_value|pk
            for cols in &rows {
                if cols.len() >= 6 {
                    let name = &cols[1];
                    let data_type = &cols[2];
                    let nullable = if cols[3] == "0" { ", nullable" } else { "" };
                    let pk = if cols[5] != "0" { ", PK" } else { "" };
                    summary.push_str(&format!("  - {name}: {data_type}{nullable}{pk}\n"));
                }
            }
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> SQLiteProvider {
        SQLiteProvider::new("/tmp/test.db")
    }

    #[test]
    fn test_db_type() {
        let p = make_provider();
        assert_eq!(p.db_type(), DatabaseType::SQLite);
    }

    #[test]
    fn test_db_path() {
        let p = make_provider();
        assert_eq!(p.db_path(), "/tmp/test.db");
    }

    #[test]
    fn test_db_path_from_string() {
        let path = String::from("/var/data/app.sqlite");
        let p = SQLiteProvider::new(path);
        assert_eq!(p.db_path(), "/var/data/app.sqlite");
    }

    #[test]
    fn test_new_with_str() {
        let p = SQLiteProvider::new(":memory:");
        assert_eq!(p.db_path(), ":memory:");
    }
}
