use crate::domain::transport::{TransportService, TransportSnapshot, TransportSnapshotInput};
use crate::infra::local_transport_service::LocalTransportService;

pub fn load_transport_snapshot(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: TransportSnapshotInput,
) -> Result<TransportSnapshot, String> {
    let service = LocalTransportService::new(app_handle);
    service.load_snapshot(input)
}
