use crate::domain::transport_runtime::TransportRuntime;
use crate::infra::mock_transport_runtime::MockTransportRuntime;
use crate::infra::preview_transport_runtime::PreviewTransportRuntime;

static MOCK_RUNTIME: MockTransportRuntime = MockTransportRuntime;
static PREVIEW_RUNTIME: PreviewTransportRuntime = PreviewTransportRuntime;

pub fn select_transport_runtime(experimental_transport: bool) -> &'static dyn TransportRuntime {
    if experimental_transport {
        &PREVIEW_RUNTIME
    } else {
        &MOCK_RUNTIME
    }
}
