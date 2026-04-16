use crate::app::{transport_mutations, transport_queries};
use crate::domain::transport::{
    TransportCircleActionInput, TransportMutationResult, TransportSnapshot, TransportSnapshotInput,
};

#[tauri::command]
pub fn load_transport_snapshot(
    app_handle: tauri::AppHandle,
    input: TransportSnapshotInput,
) -> Result<TransportSnapshot, String> {
    transport_queries::load_transport_snapshot(&app_handle, input)
}

#[tauri::command]
pub fn apply_transport_circle_action(
    app_handle: tauri::AppHandle,
    input: TransportCircleActionInput,
) -> Result<TransportMutationResult, String> {
    transport_mutations::apply_transport_circle_action(&app_handle, input)
}
