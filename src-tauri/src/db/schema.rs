use crate::error::AppError;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS usage_records (
          request_id TEXT PRIMARY KEY,
          app_type TEXT NOT NULL DEFAULT 'zcode',
          provider_id TEXT NOT NULL,
          model_id TEXT NOT NULL,
          input_tokens INTEGER NOT NULL DEFAULT 0,
          output_tokens INTEGER NOT NULL DEFAULT 0,
          reasoning_tokens INTEGER NOT NULL DEFAULT 0,
          cache_read_tokens INTEGER NOT NULL DEFAULT 0,
          cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
          input_cost_usd TEXT NOT NULL DEFAULT '0',
          output_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
          total_cost_usd TEXT NOT NULL DEFAULT '0',
          priced INTEGER NOT NULL DEFAULT 0,
          started_at INTEGER NOT NULL,
          duration_ms INTEGER,
          first_token_ms INTEGER,
          status TEXT NOT NULL,
          session_id TEXT,
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_started ON usage_records(started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model_id);
        CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_records(provider_id);

        CREATE TABLE IF NOT EXISTS sync_cursors (
          source TEXT PRIMARY KEY,
          last_started_at INTEGER NOT NULL DEFAULT 0,
          last_mtime INTEGER NOT NULL DEFAULT 0,
          last_synced_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS pricing_overrides (
          model_id TEXT PRIMARY KEY,
          input_cost_per_million TEXT NOT NULL,
          output_cost_per_million TEXT NOT NULL,
          cache_read_cost_per_million TEXT NOT NULL,
          cache_creation_cost_per_million TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}
