use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const PENDING_AUTH_RUNTIME_CLIENT_KEY: &str = "pending_auth_runtime_client";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPendingAuthRuntimeClient {
    pub public_key: String,
    pub secret_key_hex: String,
    #[serde(default)]
    pub relays: Vec<String>,
    pub client_name: String,
    pub stored_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired_bunker_uri: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<StoredPendingAuthRuntimeClient>, String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;

    conn.query_row(
        "SELECT value FROM app_kv WHERE key = ?1",
        [PENDING_AUTH_RUNTIME_CLIENT_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| {
        serde_json::from_str::<StoredPendingAuthRuntimeClient>(&value)
            .map_err(|error| error.to_string())
    })
    .transpose()
}

pub fn save<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    client: &StoredPendingAuthRuntimeClient,
) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    let value = serde_json::to_string_pretty(client).map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT INTO app_kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![PENDING_AUTH_RUNTIME_CLIENT_KEY, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn clear<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<(), String> {
    let conn = open_connection(app_handle)?;
    ensure_schema(&conn)?;
    conn.execute(
        "DELETE FROM app_kv WHERE key = ?1",
        [PENDING_AUTH_RUNTIME_CLIENT_KEY],
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
