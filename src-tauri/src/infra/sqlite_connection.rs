use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

fn app_config_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;

    fs::create_dir_all(&app_config_dir).map_err(|error| error.to_string())?;

    Ok(app_config_dir)
}

pub fn sqlite_database_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_config_dir(app_handle)?.join("p2p-chat.db"))
}

pub fn legacy_shell_state_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_config_dir(app_handle)?.join("shell-state.json"))
}

pub fn open_connection(app_handle: &tauri::AppHandle) -> Result<Connection, String> {
    let path = sqlite_database_path(app_handle)?;
    Connection::open(path).map_err(|error| error.to_string())
}
