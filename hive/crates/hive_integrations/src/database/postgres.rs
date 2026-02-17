//! PostgreSQL database provider.
//!
//! Shells out to the `psql` CLI for query execution, parsing its
//! tab/pipe-separated output into the shared [`DatabaseProvider`] types.
//! This avoids pulling in heavy native driver crates while still
//! providing full schema introspection and query capabilities.

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

/// PostgreSQL provider backed by the `psql` command-line client.
pub struct PostgresProvider {
    config: DatabaseConfig,
}

impl PostgresProvider {
    /// Create a new provider from a [`DatabaseConfig`].
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Return a reference to the underlying configuration.
    pub fn config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// Build the `psql` connection URI from config fields.
    fn connection_uri(&self) -> String {
        if let Some(ref conn) = self.config.connection_string {
            return conn.clone();
        }

        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(5432);
        let database = self.config.database.as_deref().unwrap_or("postgres");
        let user = self.config.username.as_deref().unwrap_or("postgres");

        // The password is passed via the PGPASSWORD env var, not in the URI,
        // to avoid shell-escaping issues.
        format!("postgresql://{user}@{host}:{port}/{database}")
    }

    /// Build environment variables for `psql` (primarily PGPASSWORD).
    fn env_vars(&self) -> Vec<(&str, String)> {
        let mut vars = Vec::new();
        if let Some(ref password) = self.config.password {
            vars.push(("PGPASSWORD", password.clone()));
        }
        vars
    }

    /// Execute a SQL query via `psql` and return the raw stdout.
    ///
    /// Uses `-t` (tuples only, no headers/footers), `-A` (unaligned),
    /// and `-F '|'` (pipe field separator) for easy parsing.
    async fn psql_query(&self, sql: &str) -> Result<String> {
        let uri = self.connection_uri();
        let env_owned = self.env_vars();
        let env_refs: Vec<(&str, &str)> = env_owned
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();

        run_cli_command(
            "psql",
            &[&uri, "-t", "-A", "-F", "|", "-c", sql],
            &env_refs,
        )
        .await
    }

    /// Execute a SQL query via `psql` *with* column headers included.
    async fn psql_query_with_headers(&self, sql: &str) -> Result<String> {
        let uri = self.connection_uri();
        let env_owned = self.env_vars();
        let env_refs: Vec<(&str, &str)> = env_owned
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect();

        // `-A` unaligned, `-F '|'` pipe separator, no `-t` so headers are included.
        run_cli_command(
            "psql",
            &[&uri, "-A", "-F", "|", "-c", sql],
            &env_refs,
        )
        .await
    }
}

#[async_trait]
impl DatabaseProvider for PostgresProvider {
    fn db_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn test_connection(&self) -> Result<()> {
        debug!("testing PostgreSQL connection");
        self.psql_query("SELECT 1")
            .await
            .context("PostgreSQL connection test failed")?;
        Ok(())
    }

    async fn list_schemas(&self) -> Result<Vec<SchemaInfo>> {
        debug!("listing PostgreSQL schemas");

        let sql = "\
            SELECT n.nspname AS schema_name, \
                   COUNT(c.relname) AS table_count \
            FROM pg_namespace n \
            LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind = 'r' \
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast') \
              AND n.nspname NOT LIKE 'pg_temp_%' \
              AND n.nspname NOT LIKE 'pg_toast_temp_%' \
            GROUP BY n.nspname \
            ORDER BY n.nspname";

        let output = self.psql_query(sql).await?;
        let rows = parse_separated_output(&output, '|');

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
        debug!(schema = %schema, "listing PostgreSQL tables");

        let sql = format!(
            "SELECT c.relname AS table_name, \
                    n.nspname AS schema_name, \
                    c.reltuples::bigint AS row_estimate, \
                    pg_relation_size(c.oid) AS size_bytes \
             FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = '{schema}' \
               AND c.relkind = 'r' \
             ORDER BY c.relname"
        );

        let output = self.psql_query(&sql).await?;
        let rows = parse_separated_output(&output, '|');

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
        debug!(schema = %schema, table = %table, "describing PostgreSQL table");

        // ── Columns ──────────────────────────────────────────────
        let col_sql = format!(
            "SELECT c.column_name, \
                    c.data_type, \
                    c.is_nullable, \
                    c.column_default, \
                    CASE WHEN pk.column_name IS NOT NULL THEN 'YES' ELSE 'NO' END AS is_pk \
             FROM information_schema.columns c \
             LEFT JOIN ( \
                 SELECT ku.column_name \
                 FROM information_schema.table_constraints tc \
                 JOIN information_schema.key_column_usage ku \
                   ON tc.constraint_name = ku.constraint_name \
                  AND tc.table_schema = ku.table_schema \
                 WHERE tc.constraint_type = 'PRIMARY KEY' \
                   AND tc.table_schema = '{schema}' \
                   AND tc.table_name = '{table}' \
             ) pk ON pk.column_name = c.column_name \
             WHERE c.table_schema = '{schema}' \
               AND c.table_name = '{table}' \
             ORDER BY c.ordinal_position"
        );

        let col_output = self.psql_query(&col_sql).await?;
        let col_rows = parse_separated_output(&col_output, '|');

        let mut columns = Vec::new();
        let mut primary_key = Vec::new();

        for cols in &col_rows {
            if cols.len() >= 5 {
                let is_pk = cols[4] == "YES";
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
            "SELECT kcu.column_name, \
                    ccu.table_name AS foreign_table, \
                    ccu.column_name AS foreign_column \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
              AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu \
               ON ccu.constraint_name = tc.constraint_name \
              AND ccu.table_schema = tc.table_schema \
             WHERE tc.constraint_type = 'FOREIGN KEY' \
               AND tc.table_schema = '{schema}' \
               AND tc.table_name = '{table}'"
        );

        let fk_output = self.psql_query(&fk_sql).await?;
        let fk_rows = parse_separated_output(&fk_output, '|');

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
            "SELECT i.relname AS index_name, \
                    array_to_string(array_agg(a.attname ORDER BY k.n), ',') AS columns, \
                    ix.indisunique AS is_unique \
             FROM pg_index ix \
             JOIN pg_class t ON t.oid = ix.indrelid \
             JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, n) \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum \
             WHERE n.nspname = '{schema}' \
               AND t.relname = '{table}' \
             GROUP BY i.relname, ix.indisunique \
             ORDER BY i.relname"
        );

        let idx_output = self.psql_query(&idx_sql).await?;
        let idx_rows = parse_separated_output(&idx_output, '|');

        let indexes: Vec<IndexInfo> = idx_rows
            .into_iter()
            .filter_map(|cols| {
                if cols.len() >= 3 {
                    Some(IndexInfo {
                        name: cols[0].clone(),
                        columns: cols[1].split(',').map(|s| s.trim().to_string()).collect(),
                        unique: cols[2] == "t" || cols[2] == "true",
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
        debug!(schema = %schema, table = %table, limit = limit, "sampling PostgreSQL rows");

        let sql = format!(
            "SELECT * FROM \"{schema}\".\"{table}\" LIMIT {limit}"
        );

        let output = self.psql_query_with_headers(&sql).await?;
        let lines: Vec<&str> = output.lines().collect();

        // psql with headers: first line is column names, last line is row count.
        if lines.len() < 2 {
            return Ok(Vec::new());
        }

        let headers: Vec<String> = lines[0]
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();

        let mut results = Vec::new();
        // Skip the header line and the trailing row-count line (e.g. "(5 rows)").
        for line in &lines[1..] {
            if line.starts_with('(') && line.ends_with(')') {
                continue;
            }
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
        debug!(sql = %sql, "executing PostgreSQL query");
        let start = Instant::now();

        let output = self.psql_query_with_headers(sql).await?;
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

        // Parse header row
        let columns: Vec<String> = lines[0]
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();

        let mut rows = Vec::new();
        let mut rows_affected: Option<u64> = None;

        for line in &lines[1..] {
            // Row-count footer, e.g. "(5 rows)" or "INSERT 0 3" etc.
            if line.starts_with('(') && line.ends_with(')') {
                // Extract the number from "(N rows)" or "(N row)".
                let inner = line.trim_start_matches('(').trim_end_matches(')');
                if let Some(num_str) = inner.split_whitespace().next() {
                    rows_affected = num_str.parse().ok();
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }

            let values: Vec<serde_json::Value> = line
                .split('|')
                .map(|cell| cell_to_json_value(cell))
                .collect();
            rows.push(values);
        }

        Ok(QueryResult {
            columns,
            rows,
            rows_affected,
            execution_time_ms: elapsed,
        })
    }

    async fn get_context_summary(&self) -> Result<String> {
        debug!("building PostgreSQL context summary");

        let sql = "\
            SELECT t.table_schema, \
                   t.table_name, \
                   c.column_name, \
                   c.data_type, \
                   c.is_nullable, \
                   CASE WHEN pk.column_name IS NOT NULL THEN 'PK' ELSE '' END AS is_pk \
            FROM information_schema.tables t \
            JOIN information_schema.columns c \
              ON c.table_schema = t.table_schema AND c.table_name = t.table_name \
            LEFT JOIN ( \
                SELECT ku.table_schema, ku.table_name, ku.column_name \
                FROM information_schema.table_constraints tc \
                JOIN information_schema.key_column_usage ku \
                  ON tc.constraint_name = ku.constraint_name \
                 AND tc.table_schema = ku.table_schema \
                WHERE tc.constraint_type = 'PRIMARY KEY' \
            ) pk ON pk.table_schema = c.table_schema \
               AND pk.table_name = c.table_name \
               AND pk.column_name = c.column_name \
            WHERE t.table_type = 'BASE TABLE' \
              AND t.table_schema NOT IN ('pg_catalog', 'information_schema') \
            ORDER BY t.table_schema, t.table_name, c.ordinal_position";

        let output = self.psql_query(sql).await?;
        let rows = parse_separated_output(&output, '|');

        if rows.is_empty() {
            return Ok(String::from("(No user tables found)"));
        }

        // Group by schema.table
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
            let pk_marker = &cols[5];

            let full_table = format!("{schema}.{table}");
            if full_table != current_table {
                if !current_table.is_empty() {
                    summary.push('\n');
                }
                summary.push_str(&format!("Table: {full_table}\n"));
                current_table = full_table;
            }

            let null_marker = if nullable == "YES" { ", nullable" } else { "" };
            let pk = if pk_marker == "PK" { ", PK" } else { "" };
            summary.push_str(&format!("  - {column}: {data_type}{null_marker}{pk}\n"));
        }

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> PostgresProvider {
        PostgresProvider::new(DatabaseConfig::postgres(
            "localhost", 5432, "testdb", "user", "pass",
        ))
    }

    #[test]
    fn test_db_type() {
        let p = make_provider();
        assert_eq!(p.db_type(), DatabaseType::PostgreSQL);
    }

    #[test]
    fn test_connection_uri_from_components() {
        let p = make_provider();
        let uri = p.connection_uri();
        assert_eq!(uri, "postgresql://user@localhost:5432/testdb");
    }

    #[test]
    fn test_connection_uri_from_connection_string() {
        let p = PostgresProvider::new(DatabaseConfig::from_connection_string(
            DatabaseType::PostgreSQL,
            "postgresql://admin:secret@db.host:5433/prod",
        ));
        assert_eq!(
            p.connection_uri(),
            "postgresql://admin:secret@db.host:5433/prod"
        );
    }

    #[test]
    fn test_connection_uri_defaults() {
        let p = PostgresProvider::new(DatabaseConfig {
            db_type: DatabaseType::PostgreSQL,
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            connection_string: None,
        });
        assert_eq!(p.connection_uri(), "postgresql://postgres@localhost:5432/postgres");
    }

    #[test]
    fn test_env_vars_with_password() {
        let p = make_provider();
        let vars = p.env_vars();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "PGPASSWORD");
        assert_eq!(vars[0].1, "pass");
    }

    #[test]
    fn test_env_vars_without_password() {
        let p = PostgresProvider::new(DatabaseConfig {
            db_type: DatabaseType::PostgreSQL,
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("db".into()),
            username: Some("user".into()),
            password: None,
            connection_string: None,
        });
        let vars = p.env_vars();
        assert!(vars.is_empty());
    }

    #[test]
    fn test_config_accessor() {
        let p = make_provider();
        assert_eq!(p.config().db_type, DatabaseType::PostgreSQL);
        assert_eq!(p.config().host.as_deref(), Some("localhost"));
    }
}
