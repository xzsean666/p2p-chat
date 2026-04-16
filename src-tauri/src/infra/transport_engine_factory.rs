use crate::domain::transport_engine::TransportEngine;
use crate::infra::mock_transport_engine::MockTransportEngine;
use crate::infra::preview_transport_engine::PreviewTransportEngine;

static MOCK_ENGINE: MockTransportEngine = MockTransportEngine;
static PREVIEW_ENGINE: PreviewTransportEngine = PreviewTransportEngine;

pub fn select_transport_engine(experimental_transport: bool) -> &'static dyn TransportEngine {
    if experimental_transport {
        &PREVIEW_ENGINE
    } else {
        &MOCK_ENGINE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::transport::TransportEngineKind;

    #[test]
    fn selects_preview_engine_when_experimental_transport_is_enabled() {
        assert!(matches!(
            select_transport_engine(true).kind(),
            TransportEngineKind::NativePreview
        ));
        assert!(matches!(
            select_transport_engine(false).kind(),
            TransportEngineKind::Mock
        ));
    }
}
