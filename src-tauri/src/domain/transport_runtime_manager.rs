use crate::domain::transport::{TransportChatEffects, TransportCircleActionInput};
use crate::domain::transport_repository::TransportCache;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;

pub trait TransportRuntimeManager {
    fn sync_cache(
        &self,
        previous_cache: &TransportCache,
        cache: &mut TransportCache,
        profiles: Vec<TransportRuntimeProfile>,
        action: Option<&TransportCircleActionInput>,
    ) -> Result<TransportChatEffects, String>;
}
