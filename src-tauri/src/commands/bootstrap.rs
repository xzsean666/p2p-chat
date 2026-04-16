use crate::app::bootstrap::build_bootstrap_status;
use crate::domain::bootstrap::BootstrapStatus;

#[tauri::command]
pub fn bootstrap_status() -> BootstrapStatus {
    build_bootstrap_status()
}
