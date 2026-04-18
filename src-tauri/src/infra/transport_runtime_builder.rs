use crate::domain::chat::{CircleItem, CircleStatus, SessionItem};
use crate::domain::transport::{
    RelayProtocol, TransportRuntimeRecoveryPolicy, TransportRuntimeState,
};
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_runtime_registry::{TransportRuntimeLabels, TransportRuntimeProfile};
use crate::infra::local_transport_runtime_adapter::{
    resolve_local_transport_runtime_adapter, TransportRuntimeFlavor,
};
use crate::infra::mock_transport_adapters::protocol_for_relay;

pub(crate) struct TransportRuntimeDescriptor {
    pub flavor: TransportRuntimeFlavor,
    pub recovery_policy: TransportRuntimeRecoveryPolicy,
    pub labels: TransportRuntimeLabels,
}

pub(crate) fn build_runtime_profiles(
    circles: &[CircleItem],
    sessions: &[SessionItem],
    descriptor: &TransportRuntimeDescriptor,
    options: TransportRuntimeOptions,
) -> Vec<TransportRuntimeProfile> {
    circles
        .iter()
        .map(|circle| {
            let protocol = protocol_for_relay(&circle.relay);
            let state = runtime_state(&circle.status);
            let preferred_session_id =
                preferred_preview_session_id(descriptor.flavor, &circle.id, sessions);
            let adapter = resolve_local_transport_runtime_adapter(
                descriptor.flavor,
                &protocol,
                options,
                &circle.id,
                preferred_session_id.as_deref(),
            );
            let session_label = format!(
                "{}::{}::{}",
                adapter.session_prefix, adapter.protocol_token, circle.id
            );
            let endpoint = format!(
                "{}://{}/{}",
                adapter.endpoint_scheme,
                endpoint_segment(&protocol),
                circle.id
            );

            TransportRuntimeProfile {
                circle_id: circle.id.clone(),
                driver: adapter.driver,
                adapter_kind: adapter.adapter_kind,
                launch_status: adapter.launch_status,
                launch_command: adapter.launch_command,
                launch_arguments: adapter.launch_arguments,
                resolved_launch_command: adapter.resolved_launch_command,
                launch_error: adapter.launch_error,
                recovery_policy: descriptor.recovery_policy.clone(),
                state,
                session_label,
                endpoint,
                labels: descriptor.labels,
            }
        })
        .collect()
}

fn preferred_preview_session_id(
    flavor: TransportRuntimeFlavor,
    circle_id: &str,
    sessions: &[SessionItem],
) -> Option<String> {
    if !matches!(flavor, TransportRuntimeFlavor::Preview) {
        return None;
    }

    sessions
        .iter()
        .find(|session| session.circle_id == circle_id && !session.archived.unwrap_or(false))
        .map(|session| session.id.clone())
}

fn runtime_state(status: &CircleStatus) -> TransportRuntimeState {
    match status {
        CircleStatus::Open => TransportRuntimeState::Active,
        CircleStatus::Connecting => TransportRuntimeState::Starting,
        CircleStatus::Closed => TransportRuntimeState::Inactive,
    }
}

fn endpoint_segment(protocol: &RelayProtocol) -> &'static str {
    match protocol {
        RelayProtocol::Websocket => "relay",
        RelayProtocol::Mesh => "mesh",
        RelayProtocol::Invite => "invite",
    }
}
