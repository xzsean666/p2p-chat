use crate::domain::transport::{
    TransportCircleActionInput, TransportMutationResult, TransportService,
};
use crate::infra::local_transport_service::LocalTransportService;

pub fn apply_transport_circle_action(
    app_handle: &tauri::AppHandle,
    input: TransportCircleActionInput,
) -> Result<TransportMutationResult, String> {
    let service = LocalTransportService::new(app_handle);
    service.apply_circle_action(input)
}
