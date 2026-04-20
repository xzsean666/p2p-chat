use crate::domain::chat::{LoginAccessKind, LoginMethod};
use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const AUTH_RUNTIME_BINDING_KEY: &str = "auth_runtime_binding";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAuthRuntimeBinding {
    pub login_method: LoginMethod,
    pub access_kind: LoginAccessKind,
    pub value: String,
    pub stored_at: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<StoredAuthRuntimeBinding>, String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;

    conn.query_row(
        "SELECT value FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_BINDING_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| {
        serde_json::from_str::<StoredAuthRuntimeBinding>(&value).map_err(|error| error.to_string())
    })
    .transpose()
}

pub fn save<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    binding: &StoredAuthRuntimeBinding,
) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    let value = serde_json::to_string_pretty(binding).map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![AUTH_RUNTIME_BINDING_KEY, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub fn clear<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    conn.execute(
        "DELETE FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_BINDING_KEY],
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
