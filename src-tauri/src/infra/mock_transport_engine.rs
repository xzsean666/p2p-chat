use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport::{TransportCircleActionInput, TransportEngineKind};
use crate::domain::transport_engine::{TransportEngine, TransportEngineState};
use crate::domain::transport_repository::TransportCache;
use crate::infra::transport_state_builder::{
    apply_transport_circle_action_to_seed, build_transport_engine_state,
    build_transport_engine_state_after_action,
};

pub struct MockTransportEngine;

impl TransportEngine for MockTransportEngine {
    fn kind(&self) -> TransportEngineKind {
        TransportEngineKind::Mock
    }

    fn build_state(
        &self,
        seed: &ChatDomainSeed,
        previous_cache: &TransportCache,
    ) -> Result<TransportEngineState, String> {
        Ok(build_transport_engine_state(
            self.kind(),
            seed,
            previous_cache,
        ))
    }

    fn apply_circle_action(
        &self,
        seed: &mut ChatDomainSeed,
        previous_cache: &TransportCache,
        input: &TransportCircleActionInput,
    ) -> Result<TransportEngineState, String> {
        apply_transport_circle_action_to_seed(seed, input)?;
        build_transport_engine_state_after_action(
            self.kind(),
            seed,
            previous_cache,
            &input.circle_id,
            &input.action,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        CircleItem, CircleStatus, CircleType, ContactItem, GroupProfile, SessionItem, SessionKind,
    };
    use crate::domain::transport::{TransportActivityKind, TransportCircleAction};
    use std::collections::HashMap;

    fn build_seed() -> ChatDomainSeed {
        let mut message_store = HashMap::new();
        message_store.insert("session-1".into(), Vec::new());

        ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Mesh".into(),
                relay: "mesh://circle-1".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Closed,
                latency: "--".into(),
                description: "test".into(),
            }],
            contacts: vec![ContactItem {
                id: "contact-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                handle: "@alex".into(),
                pubkey: "pubkey".into(),
                subtitle: "friend".into(),
                bio: "bio".into(),
                online: Some(true),
                blocked: Some(false),
            }],
            sessions: vec![SessionItem {
                id: "session-1".into(),
                circle_id: "circle-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                subtitle: "hello".into(),
                time: "yesterday".into(),
                unread_count: Some(3),
                muted: None,
                pinned: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                contact_id: Some("contact-1".into()),
                archived: Some(false),
            }],
            groups: Vec::<GroupProfile>::new(),
            message_store,
        }
    }

    #[test]
    fn sync_sessions_updates_seed_and_cache() {
        let engine = MockTransportEngine;
        let mut seed = build_seed();
        let previous_cache = TransportCache::default();
        let result = engine
            .apply_circle_action(
                &mut seed,
                &previous_cache,
                &TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::SyncSessions,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: false,
                },
            )
            .expect("sync sessions should succeed");

        assert!(matches!(seed.circles[0].status, CircleStatus::Open));
        assert_eq!(seed.sessions[0].time, "synced");
        assert_eq!(seed.sessions[0].unread_count, None);
        assert_eq!(seed.message_store["session-1"].len(), 1);
        assert!(matches!(result.kind, TransportEngineKind::Mock));
        assert_eq!(result.cache.session_sync.len(), 1);
        assert_eq!(result.cache.peers.len(), 1);
        assert!(matches!(
            result.cache.activities[0].kind,
            TransportActivityKind::SyncSessions
        ));
    }
}
