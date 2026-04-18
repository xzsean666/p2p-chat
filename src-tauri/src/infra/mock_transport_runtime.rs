use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport::TransportRuntimeRecoveryPolicy;
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_runtime::TransportRuntime;
use crate::domain::transport_runtime_registry::TransportRuntimeLabels;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;
use crate::infra::local_transport_runtime_adapter::TransportRuntimeFlavor;
use crate::infra::transport_runtime_builder::{build_runtime_profiles, TransportRuntimeDescriptor};

pub struct MockTransportRuntime;

const MOCK_RUNTIME_DESCRIPTOR: TransportRuntimeDescriptor = TransportRuntimeDescriptor {
    flavor: TransportRuntimeFlavor::Mock,
    recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
    labels: TransportRuntimeLabels {
        inactive_event: "mock runtime idle",
        starting_event: "mock runtime booting",
        active_event: "mock runtime active",
        connect_event: "mock runtime handshake enqueued",
        disconnect_event: "mock runtime released",
        sync_event: "mock relay checkpoint synced",
        discover_event: "mock peer sweep queued",
        sync_sessions_event: "mock session merge queued",
    },
};

impl TransportRuntime for MockTransportRuntime {
    fn build_profiles(
        &self,
        seed: &ChatDomainSeed,
        options: TransportRuntimeOptions,
    ) -> Result<Vec<TransportRuntimeProfile>, String> {
        Ok(build_runtime_profiles(
            &seed.circles,
            &seed.sessions,
            &MOCK_RUNTIME_DESCRIPTOR,
            options,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{ChatDomainSeed, CircleItem, CircleStatus, CircleType};

    #[test]
    fn mock_runtime_builds_active_profile_for_open_circle() {
        let runtime = MockTransportRuntime;
        let seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "mesh://circle-1".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Open,
                latency: "18 ms".into(),
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
                    experimental_transport: false,
                },
            )
            .expect("mock runtime should build profiles");

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].driver, "local-mock-mesh-daemon");
        assert!(matches!(
            profiles[0].adapter_kind,
            crate::domain::transport::TransportRuntimeAdapterKind::Embedded
        ));
        assert_eq!(profiles[0].launch_command, None);
        assert!(profiles[0].launch_arguments.is_empty());
        assert!(matches!(
            profiles[0].state,
            crate::domain::transport::TransportRuntimeState::Active
        ));
        assert!(matches!(
            profiles[0].recovery_policy,
            TransportRuntimeRecoveryPolicy::Manual
        ));
        assert_eq!(profiles[0].session_label, "mock::mesh::circle-1");
    }
}
