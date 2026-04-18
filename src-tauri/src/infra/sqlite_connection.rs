use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use tauri::{Manager, Runtime};

fn app_config_dir<R: Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    let app_config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;

    fs::create_dir_all(&app_config_dir).map_err(|error| error.to_string())?;

    Ok(app_config_dir)
}

pub fn sqlite_database_path<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<PathBuf, String> {
    Ok(app_config_dir(app_handle)?.join("p2p-chat.db"))
}

pub fn legacy_shell_state_path<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<PathBuf, String> {
    Ok(app_config_dir(app_handle)?.join("shell-state.json"))
}

pub fn open_connection<R: Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<Connection, String> {
    let path = sqlite_database_path(app_handle)?;
    Connection::open(path).map_err(|error| error.to_string())
}
