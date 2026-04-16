use crate::infra::shell_state_store;
use serde_json::Value;

#[tauri::command]
pub fn load_shell_state(app_handle: tauri::AppHandle) -> Result<Option<Value>, String> {
    shell_state_store::load(&app_handle)
}

#[tauri::command]
pub fn save_shell_state(app_handle: tauri::AppHandle, state: Value) -> Result<(), String> {
    shell_state_store::save(&app_handle, state)
}
