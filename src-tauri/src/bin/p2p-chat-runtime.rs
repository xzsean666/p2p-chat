use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewRuntimeMode {
    Relay,
    Mesh,
    Invite,
}

impl PreviewRuntimeMode {
    fn from_command(command: &str) -> Result<Self, String> {
        match command {
            "preview-relay" => Ok(Self::Relay),
            "preview-mesh" => Ok(Self::Mesh),
            "preview-invite" => Ok(Self::Invite),
            _ => Err(format!("unsupported command `{command}`")),
        }
    }

    fn message_body(self, use_tor_network: bool, circle_id: &str) -> String {
        match self {
            Self::Relay if use_tor_network => {
                format!("Preview relay for `{circle_id}` booted through the privacy path.")
            }
            Self::Relay => {
                format!("Preview relay for `{circle_id}` booted through the local runtime.")
            }
            Self::Mesh => {
                format!("Preview mesh runtime for `{circle_id}` published a peer update.")
            }
            Self::Invite => {
                format!("Preview invite runtime for `{circle_id}` published a join hint.")
            }
        }
    }

    fn message_suffix(self) -> &'static str {
        match self {
            Self::Relay => "relay",
            Self::Mesh => "mesh",
            Self::Invite => "invite",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewRuntimeOptions {
    mode: PreviewRuntimeMode,
    circle_id: String,
    session_id: Option<String>,
    use_tor_network: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let behavior = parse_args(env::args().skip(1))?;
    match behavior {
        CommandBehavior::Help => {
            print_usage();
            Ok(())
        }
        CommandBehavior::Run(options) => {
            emit_startup_events(&options)?;
            hold_process(&options)?;
            Ok(())
        }
    }
}

#[derive(Debug)]
enum CommandBehavior {
    Help,
    Run(PreviewRuntimeOptions),
}

fn parse_args<I>(args: I) -> Result<CommandBehavior, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(CommandBehavior::Help);
    };
    if matches!(command.as_str(), "--help" | "-h") {
        return Ok(CommandBehavior::Help);
    }

    let mode = PreviewRuntimeMode::from_command(&command)?;
    let mut circle_id = None;
    let mut session_id = None;
    let mut use_tor_network = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(CommandBehavior::Help),
            "--circle" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for `--circle`".to_string())?;
                circle_id = Some(value);
            }
            "--session" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for `--session`".to_string())?;
                session_id = Some(value);
            }
            "--tor" => {
                use_tor_network = true;
            }
            _ => {
                return Err(format!("unsupported argument `{argument}`"));
            }
        }
    }

    if use_tor_network && mode != PreviewRuntimeMode::Relay {
        return Err("`--tor` is only supported for `preview-relay`".into());
    }

    let circle_id = circle_id.ok_or_else(|| "missing required `--circle`".to_string())?;
    Ok(CommandBehavior::Run(PreviewRuntimeOptions {
        mode,
        circle_id,
        session_id,
        use_tor_network,
    }))
}

fn print_usage() {
    println!("p2p-chat-runtime preview-relay --circle <id> [--session <id>] [--tor]");
    println!("p2p-chat-runtime preview-mesh --circle <id> [--session <id>]");
    println!("p2p-chat-runtime preview-invite --circle <id> [--session <id>]");
}

fn emit_startup_events(options: &PreviewRuntimeOptions) -> Result<(), String> {
    let Some(event) = build_startup_event(options) else {
        return Ok(());
    };
    emit_runtime_output_event(&event)
}

fn build_startup_event(options: &PreviewRuntimeOptions) -> Option<TransportRuntimeOutputEvent> {
    let session_id = options.session_id.as_deref()?;
    Some(TransportRuntimeOutputEvent::MergeRemoteMessages(
        MergeRemoteMessagesInput {
            session_id: session_id.to_string(),
            messages: vec![MessageItem {
                id: format!(
                    "runtime-{}-{}-bootstrap",
                    options.mode.message_suffix(),
                    sanitize_identifier(&options.circle_id)
                ),
                kind: MessageKind::Text,
                author: MessageAuthor::Peer,
                body: options
                    .mode
                    .message_body(options.use_tor_network, &options.circle_id),
                time: "now".into(),
                meta: None,
                delivery_status: None,
                remote_id: Some(format!(
                    "runtime:{}:{}:bootstrap",
                    options.mode.message_suffix(),
                    options.circle_id
                )),
                sync_source: Some(MessageSyncSource::Relay),
                acked_at: None,
            }],
        },
    ))
}

fn emit_runtime_output_event(event: &TransportRuntimeOutputEvent) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, event)
        .map_err(|error| format!("failed to encode preview runtime event: {error}"))?;
    stdout
        .write_all(b"\n")
        .and_then(|_| stdout.flush())
        .map_err(|error| format!("failed to flush preview runtime event: {error}"))?;
    Ok(())
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn hold_process(options: &PreviewRuntimeOptions) -> Result<(), String> {
    let mut read_offset = 0u64;
    let mut request_sequence = 0u64;
    let request_queue_path = runtime_request_queue_path(&options.circle_id);

    loop {
        let (events, next_read_offset) =
            read_runtime_input_events(&request_queue_path, read_offset)?;
        read_offset = next_read_offset;
        for event in events {
            request_sequence = handle_runtime_input_event(options, event, request_sequence)?;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn handle_runtime_input_event(
    options: &PreviewRuntimeOptions,
    event: TransportRuntimeInputEvent,
    request_sequence: u64,
) -> Result<u64, String> {
    let TransportRuntimeInputEvent::ApplyCircleAction(request) = event;
    let next_sequence = request_sequence.saturating_add(1);
    let events = build_action_output_events(options, &request, next_sequence);
    if events.is_empty() {
        return Ok(request_sequence);
    }

    for runtime_event in &events {
        emit_runtime_output_event(runtime_event)?;
    }

    Ok(next_sequence)
}

fn build_action_output_events(
    options: &PreviewRuntimeOptions,
    request: &TransportRuntimeActionRequest,
    sequence: u64,
) -> Vec<TransportRuntimeOutputEvent> {
    let mut events = Vec::new();

    match &request.action {
        TransportCircleAction::SyncSessions => {
            events.push(TransportRuntimeOutputEvent::SetCircleSessionSyncState(
                CircleSessionSyncUpdate {
                    circle_id: request.circle_id.clone(),
                    state: SessionSyncState::Idle,
                    last_merge: format!(
                        "runtime merge complete for {} session(s) [{}]",
                        request.session_sync_count, request.request_id
                    ),
                },
            ));
            events.push(TransportRuntimeOutputEvent::AppendActivity {
                activity: build_runtime_activity(
                    options,
                    &request.circle_id,
                    &request.request_id,
                    sequence,
                    TransportActivityKind::SyncSessions,
                    "Preview runtime session merge committed",
                    &format!(
                        "runtime refreshed session merge state from local queue for request {} across {} session(s)",
                        request.request_id, request.session_sync_count
                    ),
                ),
            });
            if let Some(message_event) = build_runtime_message_event(
                options,
                request,
                "sync-sessions",
                format!(
                    "Preview runtime merged remote session updates for `{}` across {} session(s).",
                    request.circle_id, request.session_sync_count
                ),
            ) {
                events.push(message_event);
            }
        }
        TransportCircleAction::DiscoverPeers => {
            events.push(TransportRuntimeOutputEvent::SetCirclePeerPresence(
                CirclePeerPresenceUpdate {
                    circle_id: request.circle_id.clone(),
                    presence: if request.peer_count > 0 {
                        PeerPresence::Online
                    } else {
                        PeerPresence::Idle
                    },
                    last_seen: format!(
                        "runtime peer sweep saw {} peer(s) [{}]",
                        request.peer_count, request.request_id
                    ),
                },
            ));
            events.push(TransportRuntimeOutputEvent::AppendActivity {
                activity: build_runtime_activity(
                    options,
                    &request.circle_id,
                    &request.request_id,
                    sequence,
                    TransportActivityKind::DiscoverPeers,
                    "Preview runtime peer sweep committed",
                    &format!(
                        "runtime refreshed peer presence from local queue for request {} across {} peer(s)",
                        request.request_id, request.peer_count
                    ),
                ),
            });
            if let Some(message_event) = build_runtime_message_event(
                options,
                request,
                "discover-peers",
                format!(
                    "Preview runtime discovered {} peer(s) for `{}`.",
                    request.peer_count, request.circle_id
                ),
            ) {
                events.push(message_event);
            }
        }
        TransportCircleAction::Sync => {
            events.push(TransportRuntimeOutputEvent::SetCircleSessionSyncState(
                CircleSessionSyncUpdate {
                    circle_id: request.circle_id.clone(),
                    state: SessionSyncState::Idle,
                    last_merge: format!(
                        "runtime relay sync cleared {} unread session(s) [{}]",
                        request.unread_session_ids.len(),
                        request.request_id
                    ),
                },
            ));
            events.push(TransportRuntimeOutputEvent::AppendActivity {
                activity: build_runtime_activity(
                    options,
                    &request.circle_id,
                    &request.request_id,
                    sequence,
                    TransportActivityKind::Sync,
                    "Preview runtime relay sync committed",
                    &format!(
                        "runtime cleared {} unread marker(s) and refreshed relay state for request {}",
                        request.unread_session_ids.len(),
                        request.request_id
                    ),
                ),
            });
            events.extend(
                request
                    .unread_session_ids
                    .iter()
                    .cloned()
                    .map(|session_id| TransportRuntimeOutputEvent::ClearUnread { session_id }),
            );
        }
        TransportCircleAction::Connect | TransportCircleAction::Disconnect => {}
    }

    events
}

fn build_runtime_message_event(
    options: &PreviewRuntimeOptions,
    request: &TransportRuntimeActionRequest,
    action_token: &str,
    body: String,
) -> Option<TransportRuntimeOutputEvent> {
    let session_id = request.primary_session_id.as_deref()?;

    Some(TransportRuntimeOutputEvent::MergeRemoteMessages(
        MergeRemoteMessagesInput {
            session_id: session_id.to_string(),
            messages: vec![MessageItem {
                id: format!(
                    "runtime-{}-{}-{}-{}",
                    options.mode.message_suffix(),
                    sanitize_identifier(&request.circle_id),
                    action_token,
                    sanitize_identifier(&request.request_id)
                ),
                kind: MessageKind::Text,
                author: MessageAuthor::Peer,
                body,
                time: "now".into(),
                meta: None,
                delivery_status: None,
                remote_id: Some(format!(
                    "runtime:{}:{}:{}:{}",
                    options.mode.message_suffix(),
                    request.circle_id,
                    action_token,
                    request.request_id
                )),
                sync_source: Some(MessageSyncSource::Relay),
                acked_at: None,
            }],
        },
    ))
}

fn build_runtime_activity(
    options: &PreviewRuntimeOptions,
    circle_id: &str,
    request_id: &str,
    sequence: u64,
    kind: TransportActivityKind,
    title: &str,
    detail: &str,
) -> TransportActivityItem {
    TransportActivityItem {
        id: format!(
            "runtime-{}-{}-activity-{}-{}",
            options.mode.message_suffix(),
            sanitize_identifier(circle_id),
            sanitize_identifier(request_id),
            sequence
        ),
        circle_id: circle_id.into(),
        kind,
        level: TransportActivityLevel::Success,
        title: title.into(),
        detail: detail.into(),
        time: "now".into(),
    }
}

fn read_runtime_input_events(
    path: &Path,
    read_offset: u64,
) -> Result<(Vec<TransportRuntimeInputEvent>, u64), String> {
    let mut file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => {
            return Err(format!(
                "failed to open preview runtime input queue `{}`: {error}",
                path.display()
            ));
        }
    };

    let metadata = file.metadata().map_err(|error| {
        format!(
            "failed to inspect preview runtime input queue `{}`: {error}",
            path.display()
        )
    })?;
    let effective_offset = if read_offset > metadata.len() {
        0
    } else {
        read_offset
    };
    file.seek(SeekFrom::Start(effective_offset))
        .map_err(|error| {
            format!(
                "failed to seek preview runtime input queue `{}`: {error}",
                path.display()
            )
        })?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer).map_err(|error| {
        format!(
            "failed to read preview runtime input queue `{}`: {error}",
            path.display()
        )
    })?;

    let Some(complete_length) = buffer.rfind('\n').map(|index| index + 1) else {
        return Ok((Vec::new(), effective_offset));
    };
    let events = buffer[..complete_length]
        .lines()
        .filter_map(|line| serde_json::from_str::<TransportRuntimeInputEvent>(line).ok())
        .collect();
    Ok((events, effective_offset + complete_length as u64))
}

fn runtime_request_queue_path(circle_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("p2p-chat-runtime")
        .join("requests")
        .join(format!("{}.jsonl", sanitize_identifier(circle_id)))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum MessageKind {
    Text,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum MessageAuthor {
    Peer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum MessageSyncSource {
    Relay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PeerPresence {
    Online,
    Idle,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SessionSyncState {
    Idle,
    Syncing,
    Pending,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransportActivityKind {
    Runtime,
    Connect,
    Disconnect,
    Sync,
    DiscoverPeers,
    SyncSessions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TransportActivityLevel {
    Info,
    Success,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransportCircleAction {
    Connect,
    Disconnect,
    Sync,
    DiscoverPeers,
    SyncSessions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageItem {
    id: String,
    kind: MessageKind,
    author: MessageAuthor,
    body: String,
    time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_source: Option<MessageSyncSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeRemoteMessagesInput {
    session_id: String,
    messages: Vec<MessageItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CirclePeerPresenceUpdate {
    circle_id: String,
    presence: PeerPresence,
    last_seen: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CircleSessionSyncUpdate {
    circle_id: String,
    state: SessionSyncState,
    last_merge: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransportActivityItem {
    id: String,
    circle_id: String,
    kind: TransportActivityKind,
    level: TransportActivityLevel,
    title: String,
    detail: String,
    time: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "camelCase")]
enum TransportRuntimeOutputEvent {
    MergeRemoteMessages(MergeRemoteMessagesInput),
    ClearUnread {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    SetCirclePeerPresence(CirclePeerPresenceUpdate),
    SetCircleSessionSyncState(CircleSessionSyncUpdate),
    AppendActivity {
        activity: TransportActivityItem,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "camelCase")]
enum TransportRuntimeInputEvent {
    ApplyCircleAction(TransportRuntimeActionRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TransportRuntimeActionRequest {
    request_id: String,
    circle_id: String,
    action: TransportCircleAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unread_session_ids: Vec<String>,
    peer_count: u32,
    session_sync_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_action_request(action: TransportCircleAction) -> TransportRuntimeActionRequest {
        TransportRuntimeActionRequest {
            request_id: match action {
                TransportCircleAction::Connect => "connect:main-circle:0".into(),
                TransportCircleAction::Disconnect => "disconnect:main-circle:0".into(),
                TransportCircleAction::Sync => "sync:main-circle:1".into(),
                TransportCircleAction::DiscoverPeers => "discover-peers:main-circle:2".into(),
                TransportCircleAction::SyncSessions => "sync-sessions:main-circle:3".into(),
            },
            circle_id: "main-circle".into(),
            action,
            primary_session_id: Some("alice".into()),
            session_ids: vec!["alice".into(), "bob".into()],
            unread_session_ids: vec!["alice".into()],
            peer_count: 2,
            session_sync_count: 2,
        }
    }

    #[test]
    fn parse_args_supports_preview_relay_with_session() {
        let behavior = parse_args(vec![
            "preview-relay".to_string(),
            "--circle".to_string(),
            "main-circle".to_string(),
            "--session".to_string(),
            "alice".to_string(),
            "--tor".to_string(),
        ])
        .expect("relay arguments should parse");

        let CommandBehavior::Run(options) = behavior else {
            panic!("expected run behavior");
        };
        assert_eq!(
            options,
            PreviewRuntimeOptions {
                mode: PreviewRuntimeMode::Relay,
                circle_id: "main-circle".into(),
                session_id: Some("alice".into()),
                use_tor_network: true,
            }
        );
    }

    #[test]
    fn parse_args_rejects_tor_for_non_relay_modes() {
        let error = parse_args(vec![
            "preview-mesh".to_string(),
            "--circle".to_string(),
            "mesh-circle".to_string(),
            "--tor".to_string(),
        ])
        .expect_err("non-relay tor arguments should fail");

        assert!(error.contains("only supported for `preview-relay`"));
    }

    #[test]
    fn startup_event_serializes_matching_runtime_contract() {
        let event = TransportRuntimeOutputEvent::MergeRemoteMessages(MergeRemoteMessagesInput {
            session_id: "alice".into(),
            messages: vec![MessageItem {
                id: "runtime-relay-main-circle-bootstrap".into(),
                kind: MessageKind::Text,
                author: MessageAuthor::Peer,
                body: "Preview relay for `main-circle` booted through the local runtime.".into(),
                time: "now".into(),
                meta: None,
                delivery_status: None,
                remote_id: Some("runtime:relay:main-circle:bootstrap".into()),
                sync_source: Some(MessageSyncSource::Relay),
                acked_at: None,
            }],
        });

        let encoded = serde_json::to_string(&event).expect("runtime output event should serialize");

        assert!(encoded.contains("\"kind\":\"mergeRemoteMessages\""));
        assert!(encoded.contains("\"sessionId\":\"alice\""));
        assert!(encoded.contains("\"remoteId\":\"runtime:relay:main-circle:bootstrap\""));
        assert!(encoded.contains("\"syncSource\":\"relay\""));
    }

    #[test]
    fn build_action_output_events_maps_sync_to_clear_unread() {
        let options = PreviewRuntimeOptions {
            mode: PreviewRuntimeMode::Relay,
            circle_id: "main-circle".into(),
            session_id: Some("alice".into()),
            use_tor_network: false,
        };
        let request = runtime_action_request(TransportCircleAction::Sync);

        let events = build_action_output_events(&options, &request, 1);

        assert!(matches!(
            events.as_slice(),
            [
                TransportRuntimeOutputEvent::SetCircleSessionSyncState(
                    CircleSessionSyncUpdate { circle_id, state, last_merge }
                ),
                TransportRuntimeOutputEvent::AppendActivity { activity },
                TransportRuntimeOutputEvent::ClearUnread { session_id }
            ] if circle_id == "main-circle"
                && matches!(state, SessionSyncState::Idle)
                && last_merge == "runtime relay sync cleared 1 unread session(s) [sync:main-circle:1]"
                && activity.kind == TransportActivityKind::Sync
                && session_id == "alice"
        ));
    }

    #[test]
    fn build_action_output_events_maps_discover_peers_to_runtime_message() {
        let options = PreviewRuntimeOptions {
            mode: PreviewRuntimeMode::Relay,
            circle_id: "main-circle".into(),
            session_id: Some("alice".into()),
            use_tor_network: false,
        };
        let request = runtime_action_request(TransportCircleAction::DiscoverPeers);

        let events = build_action_output_events(&options, &request, 2);

        assert!(matches!(
            &events[0],
            TransportRuntimeOutputEvent::SetCirclePeerPresence(CirclePeerPresenceUpdate {
                circle_id,
                presence,
                last_seen
            }) if circle_id == "main-circle"
                && matches!(presence, PeerPresence::Online)
                && last_seen == "runtime peer sweep saw 2 peer(s) [discover-peers:main-circle:2]"
        ));
        assert!(matches!(
            &events[1],
            TransportRuntimeOutputEvent::AppendActivity { activity }
                if matches!(activity.kind, TransportActivityKind::DiscoverPeers)
        ));

        let TransportRuntimeOutputEvent::MergeRemoteMessages(payload) = &events[2] else {
            panic!("expected merge remote messages event");
        };
        assert_eq!(payload.session_id, "alice");
        assert_eq!(
            payload.messages[0].remote_id.as_deref(),
            Some("runtime:relay:main-circle:discover-peers:discover-peers:main-circle:2")
        );
        assert_eq!(
            payload.messages[0].body,
            "Preview runtime discovered 2 peer(s) for `main-circle`."
        );
    }

    #[test]
    fn build_action_output_events_maps_sync_sessions_to_session_sync_and_message() {
        let options = PreviewRuntimeOptions {
            mode: PreviewRuntimeMode::Relay,
            circle_id: "main-circle".into(),
            session_id: Some("alice".into()),
            use_tor_network: false,
        };
        let request = runtime_action_request(TransportCircleAction::SyncSessions);

        let events = build_action_output_events(&options, &request, 3);

        assert!(matches!(
            &events[0],
            TransportRuntimeOutputEvent::SetCircleSessionSyncState(CircleSessionSyncUpdate {
                circle_id,
                state,
                last_merge
            }) if circle_id == "main-circle"
                && matches!(state, SessionSyncState::Idle)
                && last_merge == "runtime merge complete for 2 session(s) [sync-sessions:main-circle:3]"
        ));
        assert!(matches!(
            &events[1],
            TransportRuntimeOutputEvent::AppendActivity { activity }
                if matches!(activity.kind, TransportActivityKind::SyncSessions)
        ));
        let TransportRuntimeOutputEvent::MergeRemoteMessages(payload) = &events[2] else {
            panic!("expected merge remote messages event");
        };
        assert_eq!(
            payload.messages[0].remote_id.as_deref(),
            Some("runtime:relay:main-circle:sync-sessions:sync-sessions:main-circle:3")
        );
    }

    #[test]
    fn runtime_input_event_deserializes_matching_app_contract() {
        let event = serde_json::from_str::<TransportRuntimeInputEvent>(
            r#"{"kind":"applyCircleAction","payload":{"requestId":"sync-sessions:main-circle:3","circleId":"main-circle","action":"syncSessions","primarySessionId":"alice","sessionIds":["alice","bob"],"unreadSessionIds":["alice"],"peerCount":2,"sessionSyncCount":2}}"#,
        )
        .expect("runtime input event should deserialize");

        let TransportRuntimeInputEvent::ApplyCircleAction(request) = event;
        assert_eq!(
            request,
            runtime_action_request(TransportCircleAction::SyncSessions)
        );
    }

    #[test]
    fn read_runtime_input_events_leaves_partial_line_buffered() {
        let queue_path = std::env::temp_dir().join(format!(
            "p2p-chat-runtime-test-{}.jsonl",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));

        fs::write(
            &queue_path,
            concat!(
                "{\"kind\":\"applyCircleAction\",\"payload\":{\"requestId\":\"sync-sessions:main-circle:3\",\"circleId\":\"main-circle\",\"action\":\"syncSessions\",\"primarySessionId\":\"alice\",\"sessionIds\":[\"alice\",\"bob\"],\"unreadSessionIds\":[\"alice\"],\"peerCount\":2,\"sessionSyncCount\":2}}\n",
                "{\"kind\":\"applyCircleAction\",\"payload\":{\"requestId\":\"sync-sessions:main-circle:3\",\"circleId\":\"main-circle\",\"action\":\"syncSessions\",\"primarySessionId\":\"alice\",\"sessionIds\":[\"alice\",\"bob\"],\"unreadSessionIds\":[\"alice\"],\"peerCount\":2,\"sessionSyncCount\":2}}"
            ),
        )
        .expect("queue contents should be written");

        let (events, offset) =
            read_runtime_input_events(&queue_path, 0).expect("queue should be readable");

        assert_eq!(events.len(), 1);
        let TransportRuntimeInputEvent::ApplyCircleAction(request) = &events[0];
        assert_eq!(
            request,
            &runtime_action_request(TransportCircleAction::SyncSessions)
        );
        assert!(offset > 0);

        let _ = fs::remove_file(queue_path);
    }
}
