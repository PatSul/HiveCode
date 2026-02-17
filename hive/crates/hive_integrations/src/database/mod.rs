//! Database context providers for AI-assisted development.
//!
//! Defines the [`DatabaseProvider`] trait that all database-specific
//! implementations (PostgreSQL, MySQL, SQLite) must satisfy, along with
//! common data types and the [`DatabaseHub`] registry that aggregates
//! multiple connections into a unified context for AI prompting.

pub mod mysql;
pub mod postgres;
pub mod sqlite;

use std::collections::HashMap;
use std::fmt;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

// ── Database type enum ───────────────────────────────────────────

/// Supported database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    #[serde(rename = "postgresql")]
    PostgreSQL,
    #[serde(rename = "mysql")]
    MySQL,
    #[serde(rename = "sqlite")]
    SQLite,
}

impl fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseType::PostgreSQL => write!(f, "postgresql"),
            DatabaseType::MySQL => write!(f, "mysql"),
            DatabaseType::SQLite => write!(f, "sqlite"),
        }
    }
}

// ── Configuration ────────────────────────────────────────────────

/// Connection configuration for a database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConfig {
    pub db_type: DatabaseType,
    /// Hostname or IP (not used for SQLite).
    #[serde(default)]
    pub host: Option<String>,
    /// Port number (not used for SQLite).
    #[serde(default)]
    pub port: Option<u16>,
    /// Database name or file path for SQLite.
    #[serde(default)]
    pub database: Option<String>,
    /// Username (not used for SQLite).
    #[serde(default)]
    pub username: Option<String>,
    /// Password (not used for SQLite).
    #[serde(default)]
    pub password: Option<String>,
    /// Full connection string override (e.g. `postgresql://user:pass@host/db`).
    #[serde(default)]
    pub connection_string: Option<String>,
}

impl DatabaseConfig {
    /// Build a PostgreSQL config from individual components.
    pub fn postgres(host: &str, port: u16, database: &str, username: &str, password: &str) -> Self {
        Self {
            db_type: DatabaseType::PostgreSQL,
            host: Some(host.to_string()),
            port: Some(port),
            database: Some(database.to_string()),
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            connection_string: None,
        }
    }

    /// Build a MySQL config from individual components.
    pub fn mysql(host: &str, port: u16, database: &str, username: &str, password: &str) -> Self {
        Self {
            db_type: DatabaseType::MySQL,
            host: Some(host.to_string()),
            port: Some(port),
            database: Some(database.to_string()),
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            connection_string: None,
        }
    }

    /// Build a SQLite config from a file path.
    pub fn sqlite(path: &str) -> Self {
        Self {
            db_type: DatabaseType::SQLite,
            host: None,
            port: None,
            database: Some(path.to_string()),
            username: None,
            password: None,
            connection_string: None,
        }
    }

    /// Build a config from a full connection string.
    pub fn from_connection_string(db_type: DatabaseType, conn_str: &str) -> Self {
        Self {
            db_type,
            host: None,
            port: None,
            database: None,
            username: None,
            password: None,
            connection_string: Some(conn_str.to_string()),
        }
    }
}

// ── Schema / table metadata types ────────────────────────────────

/// Summary information about a database schema (or namespace).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    pub table_count: usize,
}

/// Summary information about a single table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    pub row_count_estimate: Option<u64>,
    pub size_bytes: Option<u64>,
}

/// Detailed description of a table including columns, keys, and indexes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDescription {
    pub name: String,
    pub schema: String,
    pub columns: Vec<ColumnInfo>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<IndexInfo>,
}

/// Metadata for a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

/// A foreign-key relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
}

/// An index on a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

/// Result of executing an arbitrary SQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
}

// ── Provider trait ───────────────────────────────────────────────

/// Trait that every database integration must implement.
///
/// Implementations shell out to the corresponding CLI tool (`psql`,
/// `mysql`, `sqlite3`) via [`tokio::process::Command`] to avoid heavy
/// native driver dependencies.
#[async_trait]
pub trait DatabaseProvider: Send + Sync {
    /// Return the type of database engine backing this provider.
    fn db_type(&self) -> DatabaseType;

    /// Verify that the database is reachable.
    async fn test_connection(&self) -> Result<()>;

    /// List schemas (namespaces) available in the database.
    async fn list_schemas(&self) -> Result<Vec<SchemaInfo>>;

    /// List tables within a given schema.
    async fn list_tables(&self, schema: &str) -> Result<Vec<TableInfo>>;

    /// Return the full description of a table (columns, keys, indexes).
    async fn describe_table(&self, schema: &str, table: &str) -> Result<TableDescription>;

    /// Fetch a sample of rows from a table.
    async fn sample_rows(
        &self,
        schema: &str,
        table: &str,
        limit: u32,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>>;

    /// Execute an arbitrary SQL statement and return the results.
    async fn execute_query(&self, sql: &str) -> Result<QueryResult>;

    /// Produce a compact text summary of the database schema suitable for
    /// inclusion in an AI prompt. Lists every table with its columns and
    /// types so the model understands the available data.
    async fn get_context_summary(&self) -> Result<String>;
}

// ── Database Hub ─────────────────────────────────────────────────

/// Connection tracking entry inside the hub.
struct ConnectionEntry {
    provider: Box<dyn DatabaseProvider>,
    connected: bool,
}

/// Central registry that holds multiple named database connections.
///
/// Mirrors the pattern of [`crate::messaging::MessagingHub`] but is
/// keyed by user-chosen connection names rather than an enum variant.
pub struct DatabaseHub {
    connections: HashMap<String, ConnectionEntry>,
}

impl DatabaseHub {
    /// Create a new empty hub with no connections registered.
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Register a database provider under the given name.
    ///
    /// If a connection with the same name already exists it is replaced.
    pub fn register(&mut self, name: impl Into<String>, provider: Box<dyn DatabaseProvider>) {
        let name = name.into();
        debug!(name = %name, db_type = %provider.db_type(), "registering database provider");
        self.connections.insert(
            name,
            ConnectionEntry {
                provider,
                connected: false,
            },
        );
    }

    /// Remove a named connection. Returns `true` if it existed.
    pub fn unregister(&mut self, name: &str) -> bool {
        debug!(name = %name, "unregistering database provider");
        self.connections.remove(name).is_some()
    }

    /// Test and mark a connection as live. Returns the test result.
    pub async fn connect(&mut self, name: &str) -> Result<()> {
        let entry = self
            .connections
            .get_mut(name)
            .context(format!("no database connection registered as '{name}'"))?;

        debug!(name = %name, "testing database connection");
        entry.provider.test_connection().await?;
        entry.connected = true;
        Ok(())
    }

    /// Return a reference to the provider for `name`, if registered.
    pub fn get_provider(&self, name: &str) -> Option<&dyn DatabaseProvider> {
        self.connections.get(name).map(|e| e.provider.as_ref())
    }

    /// Return the number of registered connections.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// List all registered connections with their type and connected status.
    pub fn list_connections(&self) -> Vec<(String, DatabaseType, bool)> {
        self.connections
            .iter()
            .map(|(name, entry)| (name.clone(), entry.provider.db_type(), entry.connected))
            .collect()
    }

    /// Concatenate context summaries from all *connected* providers into a
    /// single string suitable for injection into an AI system prompt.
    pub async fn get_full_context(&self) -> String {
        let mut parts = Vec::new();

        for (name, entry) in &self.connections {
            if !entry.connected {
                continue;
            }
            match entry.provider.get_context_summary().await {
                Ok(summary) => {
                    parts.push(format!(
                        "## Database: {name} ({db_type})\n\n{summary}",
                        db_type = entry.provider.db_type(),
                    ));
                }
                Err(e) => {
                    tracing::warn!(name = %name, error = %e, "failed to get context summary");
                    parts.push(format!(
                        "## Database: {name} ({db_type})\n\n(Error retrieving schema: {e})",
                        db_type = entry.provider.db_type(),
                    ));
                }
            }
        }

        if parts.is_empty() {
            return String::from("No database connections are currently active.");
        }

        parts.join("\n\n---\n\n")
    }
}

impl Default for DatabaseHub {
    fn default() -> Self {
        Self::new()
    }
}

// ── Shared CLI helpers ───────────────────────────────────────────

/// Execute a CLI command and return its stdout as a string.
///
/// Shared by all provider implementations to reduce duplication.
pub(crate) async fn run_cli_command(
    program: &str,
    args: &[&str],
    env_vars: &[(&str, &str)],
) -> Result<String> {
    use tokio::process::Command;

    debug!(program = %program, args = ?args, "executing CLI command");

    let mut cmd = Command::new(program);
    cmd.args(args);
    for &(key, val) in env_vars {
        cmd.env(key, val);
    }
    // Prevent the CLI from trying to read a terminal.
    cmd.stdin(std::process::Stdio::null());

    let output = cmd
        .output()
        .await
        .with_context(|| format!("failed to execute '{program}' - is it installed and on PATH?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "'{program}' exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("CLI output contained invalid UTF-8")?;

    Ok(stdout)
}

/// Parse pipe-separated CLI output rows into vectors of string values.
///
/// Handles `psql -t -A -F '|'`, `mysql -B -N`, and `sqlite3 -separator '|'`
/// output formats.
pub(crate) fn parse_separated_output(output: &str, separator: char) -> Vec<Vec<String>> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split(separator)
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect()
}

/// Convert a raw string cell from CLI output to a [`serde_json::Value`].
///
/// Attempts to parse as integer, then float, then boolean, and falls
/// back to a JSON string. Recognises SQL NULL markers.
pub(crate) fn cell_to_json_value(cell: &str) -> serde_json::Value {
    let trimmed = cell.trim();

    // SQL NULL variants
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") || trimmed == "\\N" {
        return serde_json::Value::Null;
    }

    // Integer
    if let Ok(n) = trimmed.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }

    // Float
    if let Ok(f) = trimmed.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }

    // Boolean (note: "0" and "1" are already handled as integers above)
    match trimmed.to_ascii_lowercase().as_str() {
        "t" | "true" | "yes" => return serde_json::Value::Bool(true),
        "f" | "false" | "no" => return serde_json::Value::Bool(false),
        _ => {}
    }

    serde_json::Value::String(trimmed.to_string())
}

// ── Re-exports ───────────────────────────────────────────────────

pub use mysql::MySQLProvider;
pub use postgres::PostgresProvider;
pub use sqlite::SQLiteProvider;

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DatabaseType ─────────────────────────────────────────────

    #[test]
    fn test_database_type_display() {
        assert_eq!(DatabaseType::PostgreSQL.to_string(), "postgresql");
        assert_eq!(DatabaseType::MySQL.to_string(), "mysql");
        assert_eq!(DatabaseType::SQLite.to_string(), "sqlite");
    }

    #[test]
    fn test_database_type_serialize() {
        let json = serde_json::to_string(&DatabaseType::PostgreSQL).unwrap();
        assert_eq!(json, r#""postgresql""#);
        assert_eq!(serde_json::to_string(&DatabaseType::MySQL).unwrap(), r#""mysql""#);
        assert_eq!(serde_json::to_string(&DatabaseType::SQLite).unwrap(), r#""sqlite""#);
    }

    #[test]
    fn test_database_type_deserialize() {
        let dt: DatabaseType = serde_json::from_str(r#""mysql""#).unwrap();
        assert_eq!(dt, DatabaseType::MySQL);
    }

    #[test]
    fn test_database_type_roundtrip() {
        for dt in [DatabaseType::PostgreSQL, DatabaseType::MySQL, DatabaseType::SQLite] {
            let json = serde_json::to_string(&dt).unwrap();
            let back: DatabaseType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, dt);
        }
    }

    #[test]
    fn test_database_type_equality() {
        assert_eq!(DatabaseType::PostgreSQL, DatabaseType::PostgreSQL);
        assert_ne!(DatabaseType::PostgreSQL, DatabaseType::MySQL);
    }

    #[test]
    fn test_database_type_hash_as_key() {
        let mut map = HashMap::new();
        map.insert(DatabaseType::PostgreSQL, "pg");
        map.insert(DatabaseType::MySQL, "my");
        assert_eq!(map.get(&DatabaseType::PostgreSQL), Some(&"pg"));
        assert_eq!(map.get(&DatabaseType::SQLite), None);
    }

    // ── DatabaseConfig ───────────────────────────────────────────

    #[test]
    fn test_config_postgres_builder() {
        let cfg = DatabaseConfig::postgres("localhost", 5432, "mydb", "user", "pass");
        assert_eq!(cfg.db_type, DatabaseType::PostgreSQL);
        assert_eq!(cfg.host.as_deref(), Some("localhost"));
        assert_eq!(cfg.port, Some(5432));
        assert_eq!(cfg.database.as_deref(), Some("mydb"));
        assert_eq!(cfg.username.as_deref(), Some("user"));
        assert_eq!(cfg.password.as_deref(), Some("pass"));
        assert!(cfg.connection_string.is_none());
    }

    #[test]
    fn test_config_mysql_builder() {
        let cfg = DatabaseConfig::mysql("db.example.com", 3306, "shop", "admin", "secret");
        assert_eq!(cfg.db_type, DatabaseType::MySQL);
        assert_eq!(cfg.port, Some(3306));
    }

    #[test]
    fn test_config_sqlite_builder() {
        let cfg = DatabaseConfig::sqlite("/tmp/test.db");
        assert_eq!(cfg.db_type, DatabaseType::SQLite);
        assert_eq!(cfg.database.as_deref(), Some("/tmp/test.db"));
        assert!(cfg.host.is_none());
        assert!(cfg.port.is_none());
    }

    #[test]
    fn test_config_from_connection_string() {
        let cfg = DatabaseConfig::from_connection_string(
            DatabaseType::PostgreSQL,
            "postgresql://user:pass@host/db",
        );
        assert_eq!(cfg.db_type, DatabaseType::PostgreSQL);
        assert_eq!(
            cfg.connection_string.as_deref(),
            Some("postgresql://user:pass@host/db")
        );
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let cfg = DatabaseConfig::postgres("localhost", 5432, "mydb", "user", "pass");
        let json = serde_json::to_string(&cfg).unwrap();
        let back: DatabaseConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.db_type, DatabaseType::PostgreSQL);
        assert_eq!(back.host.as_deref(), Some("localhost"));
    }

    // ── Metadata types ───────────────────────────────────────────

    #[test]
    fn test_schema_info_serialization() {
        let info = SchemaInfo {
            name: "public".into(),
            table_count: 12,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SchemaInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "public");
        assert_eq!(back.table_count, 12);
    }

    #[test]
    fn test_table_info_serialization() {
        let info = TableInfo {
            name: "users".into(),
            schema: "public".into(),
            row_count_estimate: Some(1500),
            size_bytes: Some(65536),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("rowCountEstimate"));
        let back: TableInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "users");
        assert_eq!(back.row_count_estimate, Some(1500));
    }

    #[test]
    fn test_column_info_serialization() {
        let col = ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: Some("nextval('users_id_seq')".into()),
            is_primary_key: true,
        };
        let json = serde_json::to_string(&col).unwrap();
        let back: ColumnInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "id");
        assert!(!back.nullable);
        assert!(back.is_primary_key);
    }

    #[test]
    fn test_foreign_key_serialization() {
        let fk = ForeignKey {
            column: "user_id".into(),
            references_table: "users".into(),
            references_column: "id".into(),
        };
        let json = serde_json::to_string(&fk).unwrap();
        let back: ForeignKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back.column, "user_id");
        assert_eq!(back.references_table, "users");
    }

    #[test]
    fn test_index_info_serialization() {
        let idx = IndexInfo {
            name: "idx_users_email".into(),
            columns: vec!["email".into()],
            unique: true,
        };
        let json = serde_json::to_string(&idx).unwrap();
        let back: IndexInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "idx_users_email");
        assert!(back.unique);
        assert_eq!(back.columns, vec!["email"]);
    }

    #[test]
    fn test_table_description_serialization() {
        let desc = TableDescription {
            name: "orders".into(),
            schema: "public".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "user_id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            primary_key: vec!["id".into()],
            foreign_keys: vec![ForeignKey {
                column: "user_id".into(),
                references_table: "users".into(),
                references_column: "id".into(),
            }],
            indexes: vec![IndexInfo {
                name: "orders_pkey".into(),
                columns: vec!["id".into()],
                unique: true,
            }],
        };
        let json = serde_json::to_string(&desc).unwrap();
        let back: TableDescription = serde_json::from_str(&json).unwrap();
        assert_eq!(back.columns.len(), 2);
        assert_eq!(back.foreign_keys.len(), 1);
        assert_eq!(back.indexes.len(), 1);
    }

    #[test]
    fn test_query_result_serialization() {
        let qr = QueryResult {
            columns: vec!["id".into(), "name".into()],
            rows: vec![
                vec![serde_json::json!(1), serde_json::json!("Alice")],
                vec![serde_json::json!(2), serde_json::json!("Bob")],
            ],
            rows_affected: Some(2),
            execution_time_ms: 15,
        };
        let json = serde_json::to_string(&qr).unwrap();
        let back: QueryResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.columns.len(), 2);
        assert_eq!(back.rows.len(), 2);
        assert_eq!(back.rows_affected, Some(2));
        assert_eq!(back.execution_time_ms, 15);
    }

    // ── CLI helpers ──────────────────────────────────────────────

    #[test]
    fn test_parse_separated_output_pipe() {
        let output = "1|Alice|alice@example.com\n2|Bob|bob@example.com\n";
        let rows = parse_separated_output(output, '|');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
        assert_eq!(rows[1], vec!["2", "Bob", "bob@example.com"]);
    }

    #[test]
    fn test_parse_separated_output_tab() {
        let output = "users\tpublic\t100\nitems\tpublic\t50\n";
        let rows = parse_separated_output(output, '\t');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "users");
    }

    #[test]
    fn test_parse_separated_output_empty() {
        let rows = parse_separated_output("", '|');
        assert!(rows.is_empty());
    }

    #[test]
    fn test_parse_separated_output_skips_blank_lines() {
        let output = "a|b\n\nc|d\n";
        let rows = parse_separated_output(output, '|');
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_cell_to_json_value_null() {
        assert_eq!(cell_to_json_value(""), serde_json::Value::Null);
        assert_eq!(cell_to_json_value("NULL"), serde_json::Value::Null);
        assert_eq!(cell_to_json_value("null"), serde_json::Value::Null);
        assert_eq!(cell_to_json_value("\\N"), serde_json::Value::Null);
    }

    #[test]
    fn test_cell_to_json_value_integer() {
        assert_eq!(cell_to_json_value("42"), serde_json::json!(42));
        assert_eq!(cell_to_json_value("-7"), serde_json::json!(-7));
        // "0" and "1" parse as integers first (integers take priority over booleans).
        assert_eq!(cell_to_json_value("0"), serde_json::json!(0));
        assert_eq!(cell_to_json_value("1"), serde_json::json!(1));
    }

    #[test]
    fn test_cell_to_json_value_float() {
        assert_eq!(cell_to_json_value("3.14"), serde_json::json!(3.14));
        assert_eq!(cell_to_json_value("-0.5"), serde_json::json!(-0.5));
    }

    #[test]
    fn test_cell_to_json_value_boolean() {
        assert_eq!(cell_to_json_value("t"), serde_json::Value::Bool(true));
        assert_eq!(cell_to_json_value("true"), serde_json::Value::Bool(true));
        assert_eq!(cell_to_json_value("f"), serde_json::Value::Bool(false));
        assert_eq!(cell_to_json_value("false"), serde_json::Value::Bool(false));
    }

    #[test]
    fn test_cell_to_json_value_string() {
        assert_eq!(
            cell_to_json_value("hello"),
            serde_json::Value::String("hello".into())
        );
        assert_eq!(
            cell_to_json_value("2024-01-15"),
            serde_json::Value::String("2024-01-15".into())
        );
    }

    // ── DatabaseHub ──────────────────────────────────────────────

    #[test]
    fn test_hub_new_is_empty() {
        let hub = DatabaseHub::new();
        assert_eq!(hub.connection_count(), 0);
        assert!(hub.list_connections().is_empty());
    }

    #[test]
    fn test_hub_default_is_empty() {
        let hub = DatabaseHub::default();
        assert_eq!(hub.connection_count(), 0);
    }

    #[test]
    fn test_hub_register_and_get() {
        let mut hub = DatabaseHub::new();
        hub.register("test", Box::new(sqlite::SQLiteProvider::new("/tmp/test.db")));
        assert_eq!(hub.connection_count(), 1);
        assert!(hub.get_provider("test").is_some());
        assert!(hub.get_provider("other").is_none());
    }

    #[test]
    fn test_hub_register_replaces_existing() {
        let mut hub = DatabaseHub::new();
        hub.register("db", Box::new(sqlite::SQLiteProvider::new("/tmp/a.db")));
        hub.register("db", Box::new(sqlite::SQLiteProvider::new("/tmp/b.db")));
        assert_eq!(hub.connection_count(), 1);
    }

    #[test]
    fn test_hub_unregister() {
        let mut hub = DatabaseHub::new();
        hub.register("db", Box::new(sqlite::SQLiteProvider::new("/tmp/a.db")));
        assert!(hub.unregister("db"));
        assert!(!hub.unregister("db"));
        assert_eq!(hub.connection_count(), 0);
    }

    #[test]
    fn test_hub_list_connections() {
        let mut hub = DatabaseHub::new();
        hub.register("pg", Box::new(postgres::PostgresProvider::new(
            DatabaseConfig::postgres("localhost", 5432, "mydb", "user", "pass"),
        )));
        hub.register("lite", Box::new(sqlite::SQLiteProvider::new("/tmp/test.db")));

        let conns = hub.list_connections();
        assert_eq!(conns.len(), 2);

        let names: Vec<&str> = conns.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"pg"));
        assert!(names.contains(&"lite"));
    }

    #[tokio::test]
    async fn test_hub_full_context_no_connected() {
        let mut hub = DatabaseHub::new();
        hub.register("db", Box::new(sqlite::SQLiteProvider::new("/tmp/test.db")));
        let ctx = hub.get_full_context().await;
        assert_eq!(ctx, "No database connections are currently active.");
    }
}
