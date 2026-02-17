//! MySQL database provider.
//!
//! Shells out to the `mysql` CLI for query execution, parsing its
//! tab-separated output into the shared [`DatabaseProvider`] types.

use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::debug;

use super::{
    cell_to_json_value, parse_separated_output, run_cli_command, ColumnInfo, DatabaseConfig,
    DatabaseProvider, DatabaseType, ForeignKey, IndexInfo, QueryResult, SchemaInfo,
    TableDescription, TableInfo,
};

/// MySQL provider backed by the `mysql` command-line client.
pub struct MySQLProvider {
    config: DatabaseConfig,
}

impl MySQLProvider {
    /// Create a new provider from a [`DatabaseConfig`].
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Return a reference to the underlying configuration.
    pub fn config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// Build the base CLI arguments for connecting to MySQL.
    fn base_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if let Some(ref host) = self.config.host {
            args.push(format!("--host={host}"));
        }
        if let Some(port) = self.config.port {
            args.push(format!("--port={port}"));
        }
        if let Some(ref user) = self.config.username {
            args.push(format!("--user={user}"));
        }
        if let Some(ref password) = self.config.password {
            args.push(format!("--password={password}"));
        }
        if let Some(ref database) = self.config.database {
            args.push(format!("--database={database}"));
        }

        args
    }

    /// Execute a SQL query via `mysql` in batch mode and return raw stdout.
    ///
    /// Uses `-B` (batch / tab-separated) and `-N` (no column names) for
    /// easy parsing.
    async fn mysql_query(&self, sql: &str) -> Result<String> {
        let base = self.base_args();
        let mut args: Vec<&str> = base.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&["-B", "-N", "-e", sql]);

        run_cli_command("mysql", &args, &[]).await
    }

    /// Execute a SQL query via `mysql` *with* column headers.
    async fn mysql_query_with_headers(&self, sql: &str) -> Result<String> {
        let base = self.base_args();
        let mut args: Vec<&str> = base.iter().map(|s| s.as_str()).collect();
        // `-B` for tab-separated but without `-N` so headers appear on the first line.
        args.extend_from_slice(&["-B", "-e", sql]);

        run_cli_command("mysql", &args, &[]).await
    }
}

#[async_trait]
impl DatabaseProvider for MySQLProvider {
    fn db_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn test_connection(&self) -> Result<()> {
        debug!("testing MySQL connection");
        self.mysql_query("SELECT 1")
            .await
            .context("MySQL connection test failed")?;
        Ok(())
    }

    async fn list_schemas(&self) -> Result<Vec<SchemaInfo>> {
        debug!("listing MySQL schemas");

        let sql = "\
            SELECT s.schema_name, \
                   COUNT(t.table_name) AS table_count \
            FROM information_schema.schemata s \
            LEFT JOIN information_schema.tables t \
              ON t.table_schema = s.schema_name AND t.table_type = 'BASE TABLE' \
            WHERE s.schema_name NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') \
            GROUP BY s.schema_name \
            ORDER BY s.schema_name";

        let output = self.mysql_query(sql).await?;
        let rows = parse_separated_output(&output, '\t');

        Ok(rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 2 {
                    Some(SchemaInfo {
                        name: cols[0].clone(),
                        table_count: cols[1].parse().unwrap_or(0),
                    })
                } else {
                    None
                }
            })
            .collect())
    }

    async fn list_tables(&self, schema: &str) -> Result<Vec<TableInfo>> {
        debug!(schema = %schema, "listing MySQL tables");

        let sql = format!(
            "SELECT table_name, \
                    table_schema, \
                    table_rows, \
                    data_length \
             FROM information_schema.tables \
             WHERE table_schema = '{schema}' \
               AND table_type = 'BASE TABLE' \
             ORDER BY table_name"
        );

        let output = self.mysql_query(&sql).await?;
        let rows = parse_separated_output(&output, '\t');

        Ok(rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 4 {
                    Some(TableInfo {
                        name: cols[0].clone(),
                        schema: cols[1].clone(),
                        row_count_estimate: cols[2].parse().ok(),
                        size_bytes: cols[3].parse().ok(),
                    })
                } else {
                    None
                }
            })
            .collect())
    }

    async fn describe_table(&self, schema: &str, table: &str) -> Result<TableDescription> {
        debug!(schema = %schema, table = %table, "describing MySQL table");

        // ── Columns ──────────────────────────────────────────────
        let col_sql = format!(
            "SELECT c.column_name, \
                    c.column_type, \
                    c.is_nullable, \
                    c.column_default, \
                    c.column_key \
             FROM information_schema.columns c \
             WHERE c.table_schema = '{schema}' \
               AND c.table_name = '{table}' \
             ORDER BY c.ordinal_position"
        );

        let col_output = self.mysql_query(&col_sql).await?;
        let col_rows = parse_separated_output(&col_output, '\t');

        let mut columns = Vec::new();
        let mut primary_key = Vec::new();

        for cols in &col_rows {
            if cols.len() >= 5 {
                let is_pk = cols[4] == "PRI";
                let name = cols[0].clone();
                if is_pk {
                    primary_key.push(name.clone());
                }
                columns.push(ColumnInfo {
                    name,
                    data_type: cols[1].clone(),
                    nullable: cols[2] == "YES",
                    default_value: if cols[3].is_empty() || cols[3] == "NULL" {
                        None
                    } else {
                        Some(cols[3].clone())
                    },
                    is_primary_key: is_pk,
                });
            }
        }

        // ── Foreign keys ─────────────────────────────────────────
        let fk_sql = format!(
            "SELECT column_name, \
                    referenced_table_name, \
                    referenced_column_name \
             FROM information_schema.key_column_usage \
             WHERE table_schema = '{schema}' \
               AND table_name = '{table}' \
               AND referenced_table_name IS NOT NULL"
        );

        let fk_output = self.mysql_query(&fk_sql).await?;
        let fk_rows = parse_separated_output(&fk_output, '\t');

        let foreign_keys: Vec<ForeignKey> = fk_rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 3 {
                    Some(ForeignKey {
                        column: cols[0].clone(),
                        references_table: cols[1].clone(),
                        references_column: cols[2].clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        // ── Indexes ──────────────────────────────────────────────
        let idx_sql = format!(
            "SELECT index_name, \
                    GROUP_CONCAT(column_name ORDER BY seq_in_index SEPARATOR ',') AS columns, \
                    CASE WHEN non_unique = 0 THEN 'YES' ELSE 'NO' END AS is_unique \
             FROM information_schema.statistics \
             WHERE table_schema = '{schema}' \
               AND table_name = '{table}' \
             GROUP BY index_name, non_unique \
             ORDER BY index_name"
        );

        let idx_output = self.mysql_query(&idx_sql).await?;
        let idx_rows = parse_separated_output(&idx_output, '\t');

        let indexes: Vec<IndexInfo> = idx_rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 3 {
                    Some(IndexInfo {
                        name: cols[0].clone(),
                        columns: cols[1].split(',').map(|s| s.trim().to_string()).collect(),
                        unique: cols[2] == "YES",
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(TableDescription {
            name: table.to_string(),
            schema: schema.to_string(),
            columns,
            primary_key,
            foreign_keys,
            indexes,
        })
    }

    async fn sample_rows(
        &self,
        schema: &str,
        table: &str,
        limit: u32,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        debug!(schema = %schema, table = %table, limit = limit, "sampling MySQL rows");

        let sql = format!(
            "SELECT * FROM `{schema}`.`{table}` LIMIT {limit}"
        );

        let output = self.mysql_query_with_headers(&sql).await?;
        let lines: Vec<&str> = output.lines().collect();

        if lines.is_empty() {
            return Ok(Vec::new());
        }

        let headers: Vec<String> = lines[0]
            .split('\t')
            .map(|s| s.trim().to_string())
            .collect();

        let mut results = Vec::new();
        for line in &lines[1..] {
            if line.is_empty() {
                continue;
            }
            let values: Vec<&str> = line.split('\t').collect();
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
        debug!(sql = %sql, "executing MySQL query");
        let start = Instant::now();

        let output = self.mysql_query_with_headers(sql).await?;
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
            .split('\t')
            .map(|s| s.trim().to_string())
            .collect();

        let mut rows = Vec::new();
        for line in &lines[1..] {
            if line.is_empty() {
                continue;
            }
            let values: Vec<serde_json::Value> = line
                .split('\t')
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
        debug!("building MySQL context summary");

        let database = self
            .config
            .database
            .as_deref()
            .unwrap_or("(unknown)");

        let sql = format!(
            "SELECT t.table_schema, \
                    t.table_name, \
                    c.column_name, \
                    c.column_type, \
                    c.is_nullable, \
                    c.column_key \
             FROM information_schema.tables t \
             JOIN information_schema.columns c \
               ON c.table_schema = t.table_schema AND c.table_name = t.table_name \
             WHERE t.table_type = 'BASE TABLE' \
               AND t.table_schema = '{database}' \
             ORDER BY t.table_schema, t.table_name, c.ordinal_position"
        );

        let output = self.mysql_query(&sql).await?;
        let rows = parse_separated_output(&output, '\t');

        if rows.is_empty() {
            return Ok(String::from("(No user tables found)"));
        }

        let mut current_table = String::new();
        let mut summary = String::new();

        for cols in &rows {
            if cols.len() < 6 {
                continue;
            }
            let schema = &cols[0];
            let table = &cols[1];
            let column = &cols[2];
            let data_type = &cols[3];
            let nullable = &cols[4];
            let key = &cols[5];

            let full_table = format!("{schema}.{table}");
            if full_table != current_table {
                if !current_table.is_empty() {
                    summary.push('\n');
                }
                summary.push_str(&format!("Table: {full_table}\n"));
                current_table = full_table;
            }

            let null_marker = if nullable == "YES" { ", nullable" } else { "" };
            let pk = if key == "PRI" { ", PK" } else { "" };
            summary.push_str(&format!("  - {column}: {data_type}{null_marker}{pk}\n"));
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> MySQLProvider {
        MySQLProvider::new(DatabaseConfig::mysql(
            "localhost", 3306, "testdb", "root", "password",
        ))
    }

    #[test]
    fn test_db_type() {
        let p = make_provider();
        assert_eq!(p.db_type(), DatabaseType::MySQL);
    }

    #[test]
    fn test_base_args_all_fields() {
        let p = make_provider();
        let args = p.base_args();
        assert!(args.contains(&"--host=localhost".to_string()));
        assert!(args.contains(&"--port=3306".to_string()));
        assert!(args.contains(&"--user=root".to_string()));
        assert!(args.contains(&"--password=password".to_string()));
        assert!(args.contains(&"--database=testdb".to_string()));
    }

    #[test]
    fn test_base_args_minimal() {
        let p = MySQLProvider::new(DatabaseConfig {
            db_type: DatabaseType::MySQL,
            host: None,
            port: None,
            database: Some("db".into()),
            username: None,
            password: None,
            connection_string: None,
        });
        let args = p.base_args();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], "--database=db");
    }

    #[test]
    fn test_config_accessor() {
        let p = make_provider();
        assert_eq!(p.config().db_type, DatabaseType::MySQL);
        assert_eq!(p.config().host.as_deref(), Some("localhost"));
        assert_eq!(p.config().port, Some(3306));
    }
}
