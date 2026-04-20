use crate::domain::chat::{LoginAccessKind, LoginMethod};
use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const AUTH_RUNTIME_CREDENTIAL_KEY: &str = "auth_runtime_credential";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAuthRuntimeCredential {
    pub login_method: LoginMethod,
    pub access_kind: LoginAccessKind,
    pub secret_key_hex: String,
    pub pubkey: String,
    pub stored_at: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<StoredAuthRuntimeCredential>, String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;

    conn.query_row(
        "SELECT value FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_CREDENTIAL_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| {
        serde_json::from_str::<StoredAuthRuntimeCredential>(&value)
            .map_err(|error| error.to_string())
    })
    .transpose()
}

pub fn save<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    credential: &StoredAuthRuntimeCredential,
) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    let value = serde_json::to_string_pretty(credential).map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![AUTH_RUNTIME_CREDENTIAL_KEY, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub fn clear<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    conn.execute(
        "DELETE FROM app_kv WHERE key = ?1",
        [AUTH_RUNTIME_CREDENTIAL_KEY],
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
