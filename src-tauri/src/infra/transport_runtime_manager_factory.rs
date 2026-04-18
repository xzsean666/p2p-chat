use crate::domain::transport_runtime_manager::TransportRuntimeManager;
use crate::infra::local_transport_runtime_manager::LocalTransportRuntimeManager;

static LOCAL_RUNTIME_MANAGER: LocalTransportRuntimeManager = LocalTransportRuntimeManager;

pub fn select_transport_runtime_manager() -> &'static dyn TransportRuntimeManager {
    &LOCAL_RUNTIME_MANAGER
}
