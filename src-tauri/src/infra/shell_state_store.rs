use crate::infra::sqlite_connection::{legacy_shell_state_path, open_connection};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use std::fs;
const SHELL_STATE_KEY: &str = "shell_state";

pub fn load<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<Option<Value>, String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;

    let state = conn
        .query_row(
            "SELECT value FROM app_kv WHERE key = ?1",
            [SHELL_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(content) = state {
        let value = serde_json::from_str::<Value>(&content).map_err(|error| error.to_string())?;
        return Ok(Some(value));
    }

    migrate_legacy_shell_state(app_handle, &conn)
}

pub fn save<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: Value,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?;
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SHELL_STATE_KEY, content],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn ensure_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_kv (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );",
    )
    .map_err(|error| error.to_string())
}

fn migrate_legacy_shell_state(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    conn: &rusqlite::Connection,
) -> Result<Option<Value>, String> {
    let path = legacy_shell_state_path(app_handle)?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value = serde_json::from_str::<Value>(&content).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SHELL_STATE_KEY, content],
    )
    .map_err(|error| error.to_string())?;

    Ok(Some(value))
}
