//! Database Tool
//!
//! Provides SQL database operations for SQLite, PostgreSQL, and MySQL.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput};

/// Supported database types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    /// SQLite database
    Sqlite,
    /// PostgreSQL database
    Postgres,
    /// MySQL database
    Mysql,
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Type of database
    pub db_type: DatabaseType,
    /// Connection string or file path (for SQLite)
    pub connection: String,
    /// Maximum number of rows to return
    pub max_rows: usize,
    /// Whether to allow write operations
    pub allow_write: bool,
    /// Tables to allow access to (empty = all)
    pub allowed_tables: Vec<String>,
    /// Tables to deny access to
    pub denied_tables: Vec<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: DatabaseType::Sqlite,
            connection: String::new(),
            max_rows: 1000,
            allow_write: false,
            allowed_tables: vec![],
            denied_tables: vec![],
        }
    }
}

/// Database tool for executing SQL queries
pub struct DatabaseTool {
    config: DatabaseConfig,
}

impl DatabaseTool {
    /// Create a new database tool with the given configuration
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Create a SQLite database tool
    pub fn sqlite(path: impl Into<String>) -> Self {
        Self::new(DatabaseConfig {
            db_type: DatabaseType::Sqlite,
            connection: path.into(),
            ..Default::default()
        })
    }

    /// Create a read-only SQLite database tool
    pub fn sqlite_readonly(path: impl Into<String>) -> Self {
        Self::new(DatabaseConfig {
            db_type: DatabaseType::Sqlite,
            connection: path.into(),
            allow_write: false,
            ..Default::default()
        })
    }

    /// Enable write operations
    pub fn with_write_access(mut self) -> Self {
        self.config.allow_write = true;
        self
    }

    /// Set maximum rows to return
    pub fn with_max_rows(mut self, max_rows: usize) -> Self {
        self.config.max_rows = max_rows;
        self
    }

    /// Allow access to specific tables only
    pub fn with_allowed_tables(mut self, tables: Vec<String>) -> Self {
        self.config.allowed_tables = tables;
        self
    }

    /// Deny access to specific tables
    pub fn with_denied_tables(mut self, tables: Vec<String>) -> Self {
        self.config.denied_tables = tables;
        self
    }

    /// Check if a table is allowed
    fn is_table_allowed(&self, table: &str) -> bool {
        // If denied_tables contains the table, deny access
        if self.config.denied_tables.iter().any(|t| t.eq_ignore_ascii_case(table)) {
            return false;
        }
        
        // If allowed_tables is empty, allow all (except denied)
        if self.config.allowed_tables.is_empty() {
            return true;
        }
        
        // Otherwise, check if table is in allowed list
        self.config.allowed_tables.iter().any(|t| t.eq_ignore_ascii_case(table))
    }

    /// Parse SQL to extract table names (basic implementation)
    fn extract_tables_from_sql(&self, sql: &str) -> Vec<String> {
        let sql_lower = sql.to_lowercase();
        let mut tables = Vec::new();
        
        // Common patterns for table names
        let patterns = [
            "from ", "join ", "into ", "update ", "table ",
        ];
        
        for pattern in patterns {
            if let Some(idx) = sql_lower.find(pattern) {
                let start = idx + pattern.len();
                let remaining = &sql[start..];
                
                // Extract the table name (first word after pattern)
                let table: String = remaining
                    .chars()
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                
                if !table.is_empty() {
                    tables.push(table);
                }
            }
        }
        
        tables
    }

    /// Check if SQL is a read-only query
    fn is_read_only_query(&self, sql: &str) -> bool {
        let sql_trimmed = sql.trim().to_lowercase();
        sql_trimmed.starts_with("select") || 
        sql_trimmed.starts_with("pragma") ||
        sql_trimmed.starts_with("explain")
    }

    /// Validate the SQL query against configuration
    fn validate_query(&self, sql: &str) -> Result<()> {
        // Check write access
        if !self.config.allow_write && !self.is_read_only_query(sql) {
            return Err(Error::PathNotAllowed(
                "Write operations are not allowed. This database is read-only.".to_string()
            ));
        }
        
        // Check table access
        let tables = self.extract_tables_from_sql(sql);
        for table in tables {
            if !self.is_table_allowed(&table) {
                return Err(Error::PathNotAllowed(
                    format!("Access to table '{}' is not allowed.", table)
                ));
            }
        }
        
        Ok(())
    }

    /// Execute a query on SQLite database
    #[cfg(feature = "rusqlite")]
    async fn execute_sqlite(&self, sql: &str) -> Result<QueryResult> {
        use rusqlite::Connection;
        
        let conn = Connection::open(&self.config.connection)
            .map_err(|e| Error::execution_failed("database", format!("Failed to open database: {}", e)))?;
        
        let mut stmt = conn.prepare(sql)
            .map_err(|e| Error::execution_failed("database", format!("Failed to prepare statement: {}", e)))?;
        
        let column_names: Vec<String> = stmt.column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        
        let rows: Vec<HashMap<String, Value>> = stmt
            .query_map([], |row| {
                let mut map = HashMap::new();
                for (i, col_name) in column_names.iter().enumerate() {
                    let value: Value = match row.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                        Ok(rusqlite::types::ValueRef::Integer(i)) => json!(i),
                        Ok(rusqlite::types::ValueRef::Real(f)) => json!(f),
                        Ok(rusqlite::types::ValueRef::Text(s)) => json!(String::from_utf8_lossy(s)),
                        Ok(rusqlite::types::ValueRef::Blob(b)) => json!(format!("<blob: {} bytes>", b.len())),
                        Err(_) => Value::Null,
                    };
                    map.insert(col_name.clone(), value);
                }
                Ok(map)
            })
            .map_err(|e| Error::execution_failed("database", format!("Query failed: {}", e)))?
            .take(self.config.max_rows)
            .filter_map(|r| r.ok())
            .collect();
        
        Ok(QueryResult {
            columns: column_names,
            rows,
            truncated: false, // Would need to check if there were more rows
        })
    }

    /// Execute a query (fallback without SQLite feature)
    #[cfg(not(feature = "rusqlite"))]
    async fn execute_sqlite(&self, _sql: &str) -> Result<QueryResult> {
        Err(Error::ToolDisabled(
            "SQLite support is not compiled. Enable the 'sqlite' feature.".to_string()
        ))
    }
}

/// Result of a database query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Row data
    pub rows: Vec<HashMap<String, Value>>,
    /// Whether results were truncated due to max_rows limit
    pub truncated: bool,
}

impl QueryResult {
    /// Format the result as a table string
    pub fn to_table_string(&self) -> String {
        if self.columns.is_empty() || self.rows.is_empty() {
            return "No results".to_string();
        }
        
        // Calculate column widths
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.len()).collect();
        for row in &self.rows {
            for (i, col) in self.columns.iter().enumerate() {
                if let Some(value) = row.get(col) {
                    let len = format!("{}", value).len();
                    if len > widths[i] {
                        widths[i] = len.min(50); // Cap at 50 chars
                    }
                }
            }
        }
        
        let mut result = String::new();
        
        // Header
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 { result.push_str(" | "); }
            result.push_str(&format!("{:width$}", col, width = widths[i]));
        }
        result.push('\n');
        
        // Separator
        for (i, width) in widths.iter().enumerate() {
            if i > 0 { result.push_str("-+-"); }
            result.push_str(&"-".repeat(*width));
        }
        result.push('\n');
        
        // Data rows
        for row in &self.rows {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 { result.push_str(" | "); }
                let value = row.get(col).map(|v| format!("{}", v)).unwrap_or_default();
                let truncated = if value.len() > widths[i] {
                    format!("{}...", &value[..widths[i].saturating_sub(3)])
                } else {
                    value
                };
                result.push_str(&format!("{:width$}", truncated, width = widths[i]));
            }
            result.push('\n');
        }
        
        if self.truncated {
            result.push_str(&format!("\n... results truncated (showing {} rows)\n", self.rows.len()));
        }
        
        result
    }
}

#[async_trait]
impl ToolHandler for DatabaseTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new("database", "Execute SQL queries on a database. Supports SELECT queries and optionally write operations.")
            .with_parameters(json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "The operation to perform",
                        "enum": ["query", "schema", "tables"]
                    },
                    "sql": {
                        "type": "string",
                        "description": "The SQL query to execute (for 'query' operation)"
                    },
                    "table": {
                        "type": "string",
                        "description": "Table name (for 'schema' operation)"
                    }
                },
                "required": ["operation"]
            }))
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let operation = args.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("database", "Missing 'operation' argument"))?;

        match operation {
            "query" => {
                let sql = args.get("sql")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::invalid_arguments("database", "Missing 'sql' argument"))?;
                
                // Validate the query
                self.validate_query(sql)?;
                
                // Execute based on database type
                let result = match self.config.db_type {
                    DatabaseType::Sqlite => self.execute_sqlite(sql).await?,
                    DatabaseType::Postgres => {
                        return Err(Error::ToolDisabled("PostgreSQL not yet implemented".to_string()));
                    }
                    DatabaseType::Mysql => {
                        return Err(Error::ToolDisabled("MySQL not yet implemented".to_string()));
                    }
                };
                
                Ok(ToolOutput::success_with_data(
                    result.to_table_string(),
                    json!({
                        "success": true,
                        "columns": result.columns,
                        "rows": result.rows,
                        "row_count": result.rows.len(),
                        "truncated": result.truncated
                    })
                ))
            }
            "tables" => {
                // List all tables
                let sql = match self.config.db_type {
                    DatabaseType::Sqlite => "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                    DatabaseType::Postgres => "SELECT tablename FROM pg_tables WHERE schemaname = 'public'",
                    DatabaseType::Mysql => "SHOW TABLES",
                };
                
                let result = match self.config.db_type {
                    DatabaseType::Sqlite => self.execute_sqlite(sql).await?,
                    _ => return Err(Error::ToolDisabled("Only SQLite is currently implemented".to_string())),
                };
                
                let tables: Vec<String> = result.rows
                    .iter()
                    .filter_map(|row| row.values().next().and_then(|v| v.as_str().map(String::from)))
                    .filter(|t| self.is_table_allowed(t))
                    .collect();
                
                Ok(ToolOutput::success_with_data(
                    format!("Found {} tables", tables.len()),
                    json!({ "tables": tables })
                ))
            }
            "schema" => {
                let table = args.get("table")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::invalid_arguments("database", "Missing 'table' argument"))?;
                
                if !self.is_table_allowed(table) {
                    return Err(Error::PathNotAllowed(format!("Access to table '{}' is not allowed", table)));
                }
                
                let sql = match self.config.db_type {
                    DatabaseType::Sqlite => format!("PRAGMA table_info({})", table),
                    _ => return Err(Error::ToolDisabled("Only SQLite is currently implemented".to_string())),
                };
                
                let result = self.execute_sqlite(&sql).await?;
                
                Ok(ToolOutput::success_with_data(
                    format!("Schema for table '{}'", table),
                    json!({ "table": table, "schema": result.rows })
                ))
            }
            _ => Err(Error::invalid_arguments("database", format!("Unknown operation: {}", operation)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.db_type, DatabaseType::Sqlite);
        assert!(!config.allow_write);
        assert_eq!(config.max_rows, 1000);
    }

    #[test]
    fn test_table_allowed() {
        let tool = DatabaseTool::sqlite("test.db")
            .with_allowed_tables(vec!["users".to_string(), "posts".to_string()]);
        
        assert!(tool.is_table_allowed("users"));
        assert!(tool.is_table_allowed("USERS")); // Case insensitive
        assert!(!tool.is_table_allowed("secrets"));
    }

    #[test]
    fn test_table_denied() {
        let tool = DatabaseTool::sqlite("test.db")
            .with_denied_tables(vec!["secrets".to_string()]);
        
        assert!(tool.is_table_allowed("users"));
        assert!(!tool.is_table_allowed("secrets"));
        assert!(!tool.is_table_allowed("SECRETS"));
    }

    #[test]
    fn test_read_only_detection() {
        let tool = DatabaseTool::sqlite("test.db");
        
        assert!(tool.is_read_only_query("SELECT * FROM users"));
        assert!(tool.is_read_only_query("  select id from users"));
        assert!(tool.is_read_only_query("PRAGMA table_info(users)"));
        assert!(!tool.is_read_only_query("INSERT INTO users VALUES (1, 'test')"));
        assert!(!tool.is_read_only_query("UPDATE users SET name = 'test'"));
        assert!(!tool.is_read_only_query("DELETE FROM users"));
    }

    #[test]
    fn test_query_validation() {
        let tool = DatabaseTool::sqlite_readonly("test.db");
        
        // Read-only query should pass
        assert!(tool.validate_query("SELECT * FROM users").is_ok());
        
        // Write query should fail on read-only
        assert!(tool.validate_query("INSERT INTO users VALUES (1)").is_err());
    }

    #[test]
    fn test_query_result_formatting() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                {
                    let mut row = HashMap::new();
                    row.insert("id".to_string(), json!(1));
                    row.insert("name".to_string(), json!("Alice"));
                    row
                },
                {
                    let mut row = HashMap::new();
                    row.insert("id".to_string(), json!(2));
                    row.insert("name".to_string(), json!("Bob"));
                    row
                },
            ],
            truncated: false,
        };
        
        let table_str = result.to_table_string();
        assert!(table_str.contains("id"));
        assert!(table_str.contains("name"));
        assert!(table_str.contains("Alice"));
        assert!(table_str.contains("Bob"));
    }
}