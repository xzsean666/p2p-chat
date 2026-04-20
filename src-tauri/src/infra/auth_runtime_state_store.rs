use crate::domain::chat::{AuthRuntimeState, LoginAccessKind, LoginMethod};
use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const AUTH_RUNTIME_STATE_KEY: &str = "auth_runtime_state";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAuthRuntimeState {
    pub login_method: LoginMethod,
    pub access_kind: LoginAccessKind,
    pub label: String,
    pub logged_in_at: String,
    pub state: AuthRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub updated_at: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<StoredAuthRuntimeState>, String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;

    conn.query_row(
        "SELECT value FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_STATE_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| {
        serde_json::from_str::<StoredAuthRuntimeState>(&value).map_err(|error| error.to_string())
    })
    .transpose()
}

pub fn save<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    runtime: &StoredAuthRuntimeState,
) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    let value = serde_json::to_string_pretty(runtime).map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![AUTH_RUNTIME_STATE_KEY, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub fn clear<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    conn.execute(
        "DELETE FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_STATE_KEY],
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
