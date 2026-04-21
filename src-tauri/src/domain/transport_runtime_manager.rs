use crate::domain::transport::{
    TransportChatEffects, TransportCircleActionInput, TransportRelaySyncFilter,
    TransportRuntimeBackgroundSyncRequest, TransportRuntimeOutboundMedia,
    TransportRuntimeOutboundMessage,
};
use crate::domain::transport_repository::TransportCache;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;

pub trait TransportRuntimeManager {
    fn sync_cache(
        &self,
        previous_cache: &TransportCache,
        cache: &mut TransportCache,
        profiles: Vec<TransportRuntimeProfile>,
        action: Option<&TransportCircleActionInput>,
        outbound_messages: &[TransportRuntimeOutboundMessage],
        outbound_media_messages: &[TransportRuntimeOutboundMedia],
        relay_sync_filters: &[TransportRelaySyncFilter],
        background_sync_requests: &[TransportRuntimeBackgroundSyncRequest],
    ) -> Result<TransportChatEffects, String>;
}
