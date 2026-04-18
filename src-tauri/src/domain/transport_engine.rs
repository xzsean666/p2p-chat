use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport::{
    CircleTransportDiagnostic, TransportChatEffects, TransportCircleActionInput,
    TransportEngineKind, TransportHealth,
};
use crate::domain::transport_repository::TransportCache;

#[derive(Debug, Clone)]
pub struct TransportEngineState {
    pub kind: TransportEngineKind,
    pub diagnostics: Vec<CircleTransportDiagnostic>,
    pub cache: TransportCache,
    pub chat_effects: TransportChatEffects,
}

pub trait TransportEngine {
    fn kind(&self) -> TransportEngineKind;

    fn build_state(
        &self,
        seed: &ChatDomainSeed,
        previous_cache: &TransportCache,
    ) -> Result<TransportEngineState, String>;

    fn apply_circle_action(
        &self,
        seed: &mut ChatDomainSeed,
        previous_cache: &TransportCache,
        input: &TransportCircleActionInput,
    ) -> Result<TransportEngineState, String>;
}

pub fn overall_transport_health(diagnostics: &[CircleTransportDiagnostic]) -> TransportHealth {
    if diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.health, TransportHealth::Online))
    {
        return TransportHealth::Online;
    }

    if diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.health, TransportHealth::Degraded))
    {
        return TransportHealth::Degraded;
    }

    TransportHealth::Offline
}
