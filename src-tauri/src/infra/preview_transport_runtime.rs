use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport::TransportRuntimeRecoveryPolicy;
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_runtime::TransportRuntime;
use crate::domain::transport_runtime_registry::TransportRuntimeLabels;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;
use crate::infra::local_transport_runtime_adapter::TransportRuntimeFlavor;
use crate::infra::transport_runtime_builder::{build_runtime_profiles, TransportRuntimeDescriptor};

pub struct PreviewTransportRuntime;

const PREVIEW_RUNTIME_DESCRIPTOR: TransportRuntimeDescriptor = TransportRuntimeDescriptor {
    flavor: TransportRuntimeFlavor::Preview,
    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
    labels: TransportRuntimeLabels {
        inactive_event: "native runtime idle",
        starting_event: "native runtime booting",
        active_event: "native runtime active",
        connect_event: "native runtime booted",
        disconnect_event: "native runtime released",
        sync_event: "native relay checkpoint committed",
        discover_event: "native discovery sweep committed",
        sync_sessions_event: "native session merge committed",
    },
};

impl TransportRuntime for PreviewTransportRuntime {
    fn build_profiles(
        &self,
        seed: &ChatDomainSeed,
        options: TransportRuntimeOptions,
    ) -> Result<Vec<TransportRuntimeProfile>, String> {
        Ok(build_runtime_profiles(
            &seed.circles,
            &seed.sessions,
            &PREVIEW_RUNTIME_DESCRIPTOR,
            options,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        ChatDomainSeed, CircleItem, CircleStatus, CircleType, SessionItem, SessionKind,
    };

    #[test]
    fn preview_runtime_builds_native_profile() {
        let runtime = PreviewTransportRuntime;
        let seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "wss://relay.example.com".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Open,
                latency: "20 ms".into(),
                description: "test".into(),
            }],
            contacts: Vec::new(),
            sessions: vec![SessionItem {
                id: "session-1".into(),
                circle_id: "circle-1".into(),
                name: "Circle Session".into(),
                initials: "CS".into(),
                subtitle: "test".into(),
                time: "now".into(),
                unread_count: None,
                muted: None,
                pinned: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                contact_id: None,
                archived: None,
            }],
            groups: Vec::new(),
            message_store: Default::default(),
        };

        let profiles = runtime
            .build_profiles(
                &seed,
                TransportRuntimeOptions {
                    use_tor_network: false,
                    experimental_transport: true,
                },
            )
            .expect("preview runtime should build profiles");

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].driver, "native-preview-relay-runtime");
        assert!(matches!(
            profiles[0].adapter_kind,
            crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
        ));
        assert_eq!(
            profiles[0].launch_command.as_deref(),
            Some("p2p-chat-runtime")
        );
        assert_eq!(
            profiles[0].launch_arguments,
            vec![
                "preview-relay".to_string(),
                "--circle".to_string(),
                "circle-1".to_string(),
                "--session".to_string(),
                "session-1".to_string()
            ]
        );
        assert!(matches!(
            profiles[0].recovery_policy,
            TransportRuntimeRecoveryPolicy::Auto
        ));
        assert_eq!(profiles[0].endpoint, "native://relay/circle-1");
    }

    #[test]
    fn preview_runtime_uses_tor_identity_for_websocket_circle() {
        let runtime = PreviewTransportRuntime;
        let seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "wss://relay.example.com".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Connecting,
                latency: "20 ms".into(),
                description: "test".into(),
            }],
            contacts: Vec::new(),
            sessions: vec![SessionItem {
                id: "session-1".into(),
                circle_id: "circle-1".into(),
                name: "Circle Session".into(),
                initials: "CS".into(),
                subtitle: "test".into(),
                time: "now".into(),
                unread_count: None,
                muted: None,
                pinned: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                contact_id: None,
                archived: None,
            }],
            groups: Vec::new(),
            message_store: Default::default(),
        };

        let profiles = runtime
            .build_profiles(
                &seed,
                TransportRuntimeOptions {
                    use_tor_network: true,
                    experimental_transport: true,
                },
            )
            .expect("preview runtime should build tor-aware profiles");

        assert_eq!(profiles[0].driver, "native-preview-tor-runtime");
        assert!(matches!(
            profiles[0].adapter_kind,
            crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
        ));
        assert_eq!(
            profiles[0].launch_command.as_deref(),
            Some("p2p-chat-runtime")
        );
        assert_eq!(
            profiles[0].launch_arguments,
            vec![
                "preview-relay".to_string(),
                "--tor".to_string(),
                "--circle".to_string(),
                "circle-1".to_string(),
                "--session".to_string(),
                "session-1".to_string()
            ]
        );
        assert_eq!(profiles[0].session_label, "native::tor-ws::circle-1");
        assert_eq!(profiles[0].endpoint, "native+tor://relay/circle-1");
    }

    #[test]
    fn preview_runtime_skips_session_argument_when_circle_has_no_sessions() {
        let runtime = PreviewTransportRuntime;
        let seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "mesh://circle-1".into(),
                circle_type: CircleType::Bitchat,
                status: CircleStatus::Closed,
                latency: "--".into(),
                description: "test".into(),
            }],
            contacts: Vec::new(),
            sessions: Vec::new(),
            groups: Vec::new(),
            message_store: Default::default(),
        };

        let profiles = runtime
            .build_profiles(
                &seed,
                TransportRuntimeOptions {
                    use_tor_network: false,
                    experimental_transport: true,
                },
            )
            .expect("preview runtime should build mesh profiles without sessions");

        assert_eq!(
            profiles[0].launch_arguments,
            vec![
                "preview-mesh".to_string(),
                "--circle".to_string(),
                "circle-1".to_string()
            ]
        );
    }
}
