use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;
use tungstenite::{
    connect, stream::MaybeTlsStream, Error as WebSocketError, Message as WebSocketMessage,
    WebSocket,
};
use url::Url;

const RELAY_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const RELAY_SYNC_TIMEOUT: Duration = Duration::from_secs(2);
const RELAY_SYNC_LIMIT: u32 = 20;

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
    relay_url: Option<String>,
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
    let mut relay_url = None;
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
            "--relay-url" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for `--relay-url`".to_string())?;
                relay_url = Some(value);
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
        relay_url,
        session_id,
        use_tor_network,
    }))
}

fn print_usage() {
    println!(
        "p2p-chat-runtime preview-relay [--relay-url <ws[s]://...>] --circle <id> [--session <id>] [--tor]"
    );
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
                signed_nostr_event: None,
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
    let next_sequence = request_sequence.saturating_add(1);
    let events = match event {
        TransportRuntimeInputEvent::ApplyCircleAction(request) => {
            build_action_output_events(options, &request, next_sequence)
        }
        TransportRuntimeInputEvent::PublishOutboundMessages(request) => {
            build_publish_output_events(options, &request, next_sequence)
        }
    };
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

    events.extend(build_outbound_runtime_events(
        options,
        &request.circle_id,
        &request.request_id,
        &request.outbound_messages,
        sequence,
    ));
    events.extend(build_inbound_sync_output_events(options, request, sequence));

    events
}

fn build_inbound_sync_output_events(
    options: &PreviewRuntimeOptions,
    request: &TransportRuntimeActionRequest,
    sequence: u64,
) -> Vec<TransportRuntimeOutputEvent> {
    if !matches!(&request.action, TransportCircleAction::Sync) {
        return Vec::new();
    }

    if !matches!(options.mode, PreviewRuntimeMode::Relay) {
        return Vec::new();
    }

    let Some(relay_url) = options.relay_url.as_deref() else {
        return Vec::new();
    };
    let Some(session_id) = request
        .primary_session_id
        .as_deref()
        .or(options.session_id.as_deref())
    else {
        return Vec::new();
    };

    let outbound_event_ids = request
        .outbound_messages
        .iter()
        .map(|message| message.signed_nostr_event.event_id.as_str())
        .collect::<HashSet<_>>();

    match collect_relay_inbound_messages(
        relay_url,
        session_id,
        &request.request_id,
        request.sync_since_created_at,
        &request.relay_sync_filters,
        &outbound_event_ids,
    ) {
        Ok(messages) if !messages.is_empty() => {
            vec![TransportRuntimeOutputEvent::MergeRemoteMessages(
                MergeRemoteMessagesInput {
                    session_id: session_id.to_string(),
                    messages,
                },
            )]
        }
        Ok(_) => Vec::new(),
        Err(error_detail) => {
            vec![TransportRuntimeOutputEvent::AppendActivity {
                activity: build_runtime_activity_with_level(
                    options,
                    &request.circle_id,
                    &request.request_id,
                    sequence.saturating_add(200),
                    TransportActivityKind::Runtime,
                    TransportActivityLevel::Warn,
                    "Relay sync read failed",
                    &format!(
                        "runtime failed to read relay sync events for session `{session_id}`: {error_detail}"
                    ),
                ),
            }]
        }
    }
}

fn build_publish_output_events(
    options: &PreviewRuntimeOptions,
    request: &TransportRuntimePublishRequest,
    sequence: u64,
) -> Vec<TransportRuntimeOutputEvent> {
    build_outbound_runtime_events(
        options,
        &request.circle_id,
        &request.request_id,
        &request.outbound_messages,
        sequence,
    )
}

fn build_outbound_runtime_events(
    options: &PreviewRuntimeOptions,
    circle_id: &str,
    request_id: &str,
    outbound_messages: &[TransportRuntimeOutboundMessage],
    sequence: u64,
) -> Vec<TransportRuntimeOutputEvent> {
    if outbound_messages.is_empty() {
        return Vec::new();
    }

    let publish_result = if matches!(options.mode, PreviewRuntimeMode::Relay) {
        options
            .relay_url
            .as_deref()
            .map(|relay_url| publish_outbound_messages_to_relay(relay_url, outbound_messages))
            .unwrap_or_else(|| preview_outbound_publish_result(outbound_messages))
    } else {
        preview_outbound_publish_result(outbound_messages)
    };

    let mut events = build_outbound_receipt_events(&publish_result.outcomes);
    if let Some(error_detail) = publish_result.error_detail {
        events.push(TransportRuntimeOutputEvent::AppendActivity {
            activity: build_runtime_activity_with_level(
                options,
                circle_id,
                request_id,
                sequence.saturating_add(100),
                TransportActivityKind::Runtime,
                TransportActivityLevel::Warn,
                "Relay publish failed",
                &format!(
                    "runtime failed to publish {} outbound event(s): {}",
                    outbound_messages.len(),
                    error_detail
                ),
            ),
        });
    }

    events
}

fn build_outbound_receipt_events(
    outcomes: &[OutboundPublishOutcome],
) -> Vec<TransportRuntimeOutputEvent> {
    let mut receipts_by_session = BTreeMap::<String, Vec<RemoteDeliveryReceipt>>::new();

    for outbound in outcomes {
        receipts_by_session
            .entry(outbound.session_id.clone())
            .or_default()
            .push(RemoteDeliveryReceipt {
                remote_id: outbound.remote_id.clone(),
                message_id: Some(outbound.message_id.clone()),
                delivery_status: outbound.delivery_status.clone(),
                acked_at: outbound.acked_at.clone(),
            });
    }

    receipts_by_session
        .into_iter()
        .map(|(session_id, receipts)| {
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(
                MergeRemoteDeliveryReceiptsInput {
                    session_id,
                    receipts,
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone)]
struct OutboundPublishOutcome {
    session_id: String,
    message_id: String,
    remote_id: String,
    delivery_status: MessageDeliveryStatus,
    acked_at: Option<String>,
}

#[derive(Debug, Clone)]
struct OutboundPublishResult {
    outcomes: Vec<OutboundPublishOutcome>,
    error_detail: Option<String>,
}

#[derive(Debug, Clone)]
struct RelayPublishAck {
    accepted: bool,
    message: String,
}

fn preview_outbound_publish_result(
    outbound_messages: &[TransportRuntimeOutboundMessage],
) -> OutboundPublishResult {
    OutboundPublishResult {
        outcomes: outbound_messages
            .iter()
            .map(sent_outbound_publish_outcome)
            .collect(),
        error_detail: None,
    }
}

fn publish_outbound_messages_to_relay(
    relay_url: &str,
    outbound_messages: &[TransportRuntimeOutboundMessage],
) -> OutboundPublishResult {
    let relay_url = match Url::parse(relay_url) {
        Ok(relay_url) => relay_url,
        Err(error) => {
            return failed_outbound_publish_result(
                outbound_messages,
                format!("invalid relay URL `{relay_url}`: {error}"),
            );
        }
    };
    if !matches!(relay_url.scheme(), "ws" | "wss") {
        return failed_outbound_publish_result(
            outbound_messages,
            format!(
                "unsupported relay scheme `{}` for outbound relay publish",
                relay_url.scheme()
            ),
        );
    }

    let mut socket = match connect(relay_url.as_str()) {
        Ok((socket, _)) => socket,
        Err(error) => {
            return failed_outbound_publish_result(
                outbound_messages,
                format!("failed to connect to relay `{}`: {error}", relay_url),
            );
        }
    };
    if let Err(error) = configure_relay_socket_timeouts(&mut socket, RELAY_ACK_TIMEOUT) {
        return failed_outbound_publish_result(
            outbound_messages,
            format!("failed to configure relay socket timeout for `{relay_url}`: {error}"),
        );
    }
    let mut outcomes = Vec::with_capacity(outbound_messages.len());
    let mut error_detail = None;

    for (index, outbound) in outbound_messages.iter().enumerate() {
        let payload = match encode_nostr_client_event_message(&outbound.signed_nostr_event) {
            Ok(payload) => payload,
            Err(error) => {
                outcomes.push(failed_outbound_publish_outcome(outbound));
                error_detail.get_or_insert(error);
                continue;
            }
        };

        match socket.send(WebSocketMessage::Text(payload.into())) {
            Ok(()) => match await_outbound_publish_ack(&mut socket, outbound) {
                Ok(ack) if ack.accepted => outcomes.push(sent_outbound_publish_outcome(outbound)),
                Ok(ack) => {
                    outcomes.push(failed_outbound_publish_outcome(outbound));
                    error_detail.get_or_insert_with(|| {
                        format!(
                            "relay rejected event `{}`: {}",
                            outbound.remote_id, ack.message
                        )
                    });
                }
                Err(error) => {
                    outcomes.push(failed_outbound_publish_outcome(outbound));
                    error_detail.get_or_insert(error);
                    for remaining in outbound_messages.iter().skip(index + 1) {
                        outcomes.push(failed_outbound_publish_outcome(remaining));
                    }
                    break;
                }
            },
            Err(error) => {
                outcomes.push(failed_outbound_publish_outcome(outbound));
                error_detail.get_or_insert_with(|| {
                    format!(
                        "relay send failed for event `{}`: {error}",
                        outbound.remote_id
                    )
                });
                for remaining in outbound_messages.iter().skip(index + 1) {
                    outcomes.push(failed_outbound_publish_outcome(remaining));
                }
                break;
            }
        }
    }

    let _ = socket.close(None);

    OutboundPublishResult {
        outcomes,
        error_detail,
    }
}

fn collect_relay_inbound_messages(
    relay_url: &str,
    session_id: &str,
    request_id: &str,
    sync_since_created_at: Option<u64>,
    relay_sync_filters: &[TransportRelaySyncFilter],
    outbound_event_ids: &HashSet<&str>,
) -> Result<Vec<MessageItem>, String> {
    let relay_url = match Url::parse(relay_url) {
        Ok(relay_url) => relay_url,
        Err(error) => {
            return Err(format!("invalid relay URL `{relay_url}`: {error}"));
        }
    };
    if !matches!(relay_url.scheme(), "ws" | "wss") {
        return Err(format!(
            "unsupported relay scheme `{}` for inbound relay sync",
            relay_url.scheme()
        ));
    }

    let mut socket = connect(relay_url.as_str())
        .map(|(socket, _)| socket)
        .map_err(|error| format!("failed to connect to relay `{}`: {error}", relay_url))?;
    configure_relay_socket_timeouts(&mut socket, RELAY_SYNC_TIMEOUT).map_err(|error| {
        format!("failed to configure relay socket timeout for `{relay_url}`: {error}")
    })?;

    let subscription_id = format!("preview-sync-{}", sanitize_identifier(request_id));
    let request_payload = encode_nostr_relay_subscription_request(
        &subscription_id,
        sync_since_created_at,
        relay_sync_filters,
    )?;
    socket
        .send(WebSocketMessage::Text(request_payload.into()))
        .map_err(|error| {
            format!("failed to request relay sync for session `{session_id}`: {error}")
        })?;

    let mut messages = Vec::new();

    loop {
        match socket.read() {
            Ok(WebSocketMessage::Text(payload)) => {
                if let Some(frame) =
                    parse_relay_subscription_frame(payload.as_ref(), &subscription_id)?
                {
                    match frame {
                        RelaySubscriptionFrame::Event(event) => {
                            if outbound_event_ids.contains(event.id.as_str()) {
                                continue;
                            }
                            if let Some(message) = relay_event_to_message(event) {
                                messages.push(message);
                            }
                        }
                        RelaySubscriptionFrame::EndOfStoredEvents => break,
                        RelaySubscriptionFrame::Closed(reason) => {
                            return Err(format!(
                                "relay closed sync subscription `{subscription_id}`: {reason}"
                            ));
                        }
                        RelaySubscriptionFrame::Notice(reason) => {
                            return Err(format!(
                                "relay notice during sync subscription `{subscription_id}`: {reason}"
                            ));
                        }
                    }
                }
            }
            Ok(WebSocketMessage::Binary(payload)) => {
                let payload = std::str::from_utf8(&payload).map_err(|error| {
                    format!(
                        "relay returned invalid binary sync frame for session `{session_id}`: {error}"
                    )
                })?;
                if let Some(frame) = parse_relay_subscription_frame(payload, &subscription_id)? {
                    match frame {
                        RelaySubscriptionFrame::Event(event) => {
                            if outbound_event_ids.contains(event.id.as_str()) {
                                continue;
                            }
                            if let Some(message) = relay_event_to_message(event) {
                                messages.push(message);
                            }
                        }
                        RelaySubscriptionFrame::EndOfStoredEvents => break,
                        RelaySubscriptionFrame::Closed(reason) => {
                            return Err(format!(
                                "relay closed sync subscription `{subscription_id}`: {reason}"
                            ));
                        }
                        RelaySubscriptionFrame::Notice(reason) => {
                            return Err(format!(
                                "relay notice during sync subscription `{subscription_id}`: {reason}"
                            ));
                        }
                    }
                }
            }
            Ok(WebSocketMessage::Ping(_))
            | Ok(WebSocketMessage::Pong(_))
            | Ok(WebSocketMessage::Frame(_)) => {}
            Ok(WebSocketMessage::Close(_)) => break,
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => {
                return Err(format!(
                    "failed while reading relay sync frames for session `{session_id}`: {error}"
                ));
            }
        }
    }

    if let Ok(close_payload) = encode_nostr_relay_subscription_close(&subscription_id) {
        let _ = socket.send(WebSocketMessage::Text(close_payload.into()));
    }
    let _ = socket.close(None);

    Ok(messages)
}

fn configure_relay_socket_timeouts(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<(), String> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            stream
                .set_read_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
        }
        MaybeTlsStream::Rustls(stream) => {
            stream
                .sock
                .set_read_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
            stream
                .sock
                .set_write_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }

    Ok(())
}

fn await_outbound_publish_ack(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    outbound: &TransportRuntimeOutboundMessage,
) -> Result<RelayPublishAck, String> {
    loop {
        match socket.read() {
            Ok(WebSocketMessage::Text(payload)) => {
                if let Some(ack) = parse_relay_publish_ack(
                    payload.as_ref(),
                    &outbound.signed_nostr_event.event_id,
                )? {
                    return Ok(ack);
                }
            }
            Ok(WebSocketMessage::Binary(payload)) => {
                let payload = std::str::from_utf8(&payload).map_err(|error| {
                    format!(
                        "relay returned invalid binary ack for event `{}`: {error}",
                        outbound.remote_id
                    )
                })?;
                if let Some(ack) =
                    parse_relay_publish_ack(payload, &outbound.signed_nostr_event.event_id)?
                {
                    return Ok(ack);
                }
            }
            Ok(WebSocketMessage::Ping(_))
            | Ok(WebSocketMessage::Pong(_))
            | Ok(WebSocketMessage::Frame(_)) => {}
            Ok(WebSocketMessage::Close(_)) => {
                return Err(format!(
                    "relay closed connection before acknowledging event `{}`",
                    outbound.remote_id
                ));
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(format!(
                    "timed out waiting for relay OK for event `{}`",
                    outbound.remote_id
                ));
            }
            Err(error) => {
                return Err(format!(
                    "failed while waiting for relay OK for event `{}`: {error}",
                    outbound.remote_id
                ));
            }
        }
    }
}

fn parse_relay_publish_ack(
    payload: &str,
    expected_event_id: &str,
) -> Result<Option<RelayPublishAck>, String> {
    let value = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(items) = value.as_array() else {
        return Ok(None);
    };
    if items.len() < 4 || items.first().and_then(|item| item.as_str()) != Some("OK") {
        return Ok(None);
    }

    let event_id = items
        .get(1)
        .and_then(|item| item.as_str())
        .ok_or_else(|| "relay OK payload missing event id".to_string())?;
    if event_id != expected_event_id {
        return Ok(None);
    }

    let accepted = items
        .get(2)
        .and_then(|item| item.as_bool())
        .ok_or_else(|| {
            format!("relay OK payload missing acceptance flag for event `{event_id}`")
        })?;
    let message = items
        .get(3)
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Some(RelayPublishAck { accepted, message }))
}

fn encode_nostr_relay_subscription_request(
    subscription_id: &str,
    sync_since_created_at: Option<u64>,
    relay_sync_filters: &[TransportRelaySyncFilter],
) -> Result<String, String> {
    let mut payload = vec![
        serde_json::Value::String("REQ".into()),
        serde_json::Value::String(subscription_id.into()),
    ];
    let mut filters = relay_sync_filters
        .iter()
        .filter_map(|filter| {
            build_nostr_relay_subscription_filter(sync_since_created_at, Some(filter))
        })
        .collect::<Vec<_>>();
    if filters.is_empty() {
        if let Some(default_filter) =
            build_nostr_relay_subscription_filter(sync_since_created_at, None)
        {
            filters.push(default_filter);
        }
    }
    payload.extend(filters);

    serde_json::to_string(&serde_json::Value::Array(payload))
        .map_err(|error| format!("failed to encode relay subscription request: {error}"))
}

fn build_nostr_relay_subscription_filter(
    sync_since_created_at: Option<u64>,
    relay_sync_filter: Option<&TransportRelaySyncFilter>,
) -> Option<serde_json::Value> {
    let mut filter = serde_json::Map::new();
    filter.insert("kinds".into(), serde_json::json!([1]));
    filter.insert("limit".into(), serde_json::json!(RELAY_SYNC_LIMIT));
    if let Some(sync_since_created_at) = sync_since_created_at {
        filter.insert("since".into(), serde_json::json!(sync_since_created_at));
    }
    if let Some(relay_sync_filter) = relay_sync_filter {
        if relay_sync_filter.authors.is_empty() && relay_sync_filter.tagged_pubkeys.is_empty() {
            return None;
        }
        if !relay_sync_filter.authors.is_empty() {
            filter.insert(
                "authors".into(),
                serde_json::json!(relay_sync_filter.authors),
            );
        }
        if !relay_sync_filter.tagged_pubkeys.is_empty() {
            filter.insert(
                "#p".into(),
                serde_json::json!(relay_sync_filter.tagged_pubkeys),
            );
        }
    }

    Some(serde_json::Value::Object(filter))
}

fn encode_nostr_relay_subscription_close(subscription_id: &str) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!(["CLOSE", subscription_id]))
        .map_err(|error| format!("failed to encode relay subscription close: {error}"))
}

#[derive(Debug, Clone)]
enum RelaySubscriptionFrame {
    Event(RelayInboundEvent),
    EndOfStoredEvents,
    Closed(String),
    Notice(String),
}

#[derive(Debug, Clone, Deserialize)]
struct RelayInboundEvent {
    id: String,
    pubkey: String,
    created_at: u64,
    kind: u32,
    #[serde(default)]
    tags: Vec<Vec<String>>,
    #[serde(default)]
    content: String,
    #[serde(rename = "sig")]
    signature: String,
}

fn parse_relay_subscription_frame(
    payload: &str,
    expected_subscription_id: &str,
) -> Result<Option<RelaySubscriptionFrame>, String> {
    let value = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(items) = value.as_array() else {
        return Ok(None);
    };
    let Some(frame_kind) = items.first().and_then(|item| item.as_str()) else {
        return Ok(None);
    };

    match frame_kind {
        "EVENT" => {
            let subscription_id = items
                .get(1)
                .and_then(|item| item.as_str())
                .ok_or_else(|| "relay EVENT payload missing subscription id".to_string())?;
            if subscription_id != expected_subscription_id {
                return Ok(None);
            }
            let event = items
                .get(2)
                .cloned()
                .ok_or_else(|| "relay EVENT payload missing event".to_string())?;
            let event = serde_json::from_value::<RelayInboundEvent>(event)
                .map_err(|error| format!("relay EVENT payload could not be decoded: {error}"))?;
            Ok(Some(RelaySubscriptionFrame::Event(event)))
        }
        "EOSE" => {
            let subscription_id = items
                .get(1)
                .and_then(|item| item.as_str())
                .ok_or_else(|| "relay EOSE payload missing subscription id".to_string())?;
            if subscription_id != expected_subscription_id {
                return Ok(None);
            }
            Ok(Some(RelaySubscriptionFrame::EndOfStoredEvents))
        }
        "CLOSED" => {
            let subscription_id = items
                .get(1)
                .and_then(|item| item.as_str())
                .ok_or_else(|| "relay CLOSED payload missing subscription id".to_string())?;
            if subscription_id != expected_subscription_id {
                return Ok(None);
            }
            let reason = items
                .get(2)
                .and_then(|item| item.as_str())
                .unwrap_or("subscription closed")
                .to_string();
            Ok(Some(RelaySubscriptionFrame::Closed(reason)))
        }
        "NOTICE" => Ok(Some(RelaySubscriptionFrame::Notice(
            items
                .get(1)
                .and_then(|item| item.as_str())
                .unwrap_or("relay notice")
                .to_string(),
        ))),
        _ => Ok(None),
    }
}

fn relay_event_to_message(event: RelayInboundEvent) -> Option<MessageItem> {
    if event.kind != 1 {
        return None;
    }

    Some(MessageItem {
        id: event.id.clone(),
        kind: MessageKind::Text,
        author: MessageAuthor::Peer,
        body: event.content.clone(),
        time: "now".into(),
        meta: None,
        delivery_status: None,
        remote_id: Some(event.id.clone()),
        sync_source: Some(MessageSyncSource::Relay),
        acked_at: None,
        signed_nostr_event: Some(SignedNostrEvent {
            event_id: event.id,
            pubkey: event.pubkey,
            created_at: event.created_at,
            tags: event.tags,
            kind: event.kind,
            content: event.content,
            signature: event.signature,
        }),
    })
}

fn encode_nostr_client_event_message(event: &SignedNostrEvent) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!([
        "EVENT",
        {
            "id": event.event_id,
            "pubkey": event.pubkey,
            "created_at": event.created_at,
            "kind": event.kind,
            "tags": event.tags,
            "content": event.content,
            "sig": event.signature,
        }
    ]))
    .map_err(|error| format!("failed to encode nostr event payload: {error}"))
}

fn sent_outbound_publish_outcome(
    outbound: &TransportRuntimeOutboundMessage,
) -> OutboundPublishOutcome {
    OutboundPublishOutcome {
        session_id: outbound.session_id.clone(),
        message_id: outbound.message_id.clone(),
        remote_id: outbound.remote_id.clone(),
        delivery_status: MessageDeliveryStatus::Sent,
        acked_at: Some("now".into()),
    }
}

fn failed_outbound_publish_outcome(
    outbound: &TransportRuntimeOutboundMessage,
) -> OutboundPublishOutcome {
    OutboundPublishOutcome {
        session_id: outbound.session_id.clone(),
        message_id: outbound.message_id.clone(),
        remote_id: outbound.remote_id.clone(),
        delivery_status: MessageDeliveryStatus::Failed,
        acked_at: None,
    }
}

fn failed_outbound_publish_result(
    outbound_messages: &[TransportRuntimeOutboundMessage],
    error_detail: String,
) -> OutboundPublishResult {
    OutboundPublishResult {
        outcomes: outbound_messages
            .iter()
            .map(failed_outbound_publish_outcome)
            .collect(),
        error_detail: Some(error_detail),
    }
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
                signed_nostr_event: None,
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
    build_runtime_activity_with_level(
        options,
        circle_id,
        request_id,
        sequence,
        kind,
        TransportActivityLevel::Success,
        title,
        detail,
    )
}

fn build_runtime_activity_with_level(
    options: &PreviewRuntimeOptions,
    circle_id: &str,
    request_id: &str,
    sequence: u64,
    kind: TransportActivityKind,
    level: TransportActivityLevel,
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
        level,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MessageDeliveryStatus {
    Sending,
    Sent,
    Failed,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    signed_nostr_event: Option<SignedNostrEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeRemoteMessagesInput {
    session_id: String,
    messages: Vec<MessageItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteDeliveryReceipt {
    remote_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    delivery_status: MessageDeliveryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    acked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeRemoteDeliveryReceiptsInput {
    session_id: String,
    receipts: Vec<RemoteDeliveryReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SignedNostrEvent {
    event_id: String,
    pubkey: String,
    created_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<Vec<String>>,
    kind: u32,
    content: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TransportRuntimeOutboundMessage {
    session_id: String,
    message_id: String,
    remote_id: String,
    signed_nostr_event: SignedNostrEvent,
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
    MergeRemoteDeliveryReceipts(MergeRemoteDeliveryReceiptsInput),
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
    PublishOutboundMessages(TransportRuntimePublishRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TransportRelaySyncFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tagged_pubkeys: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sync_since_created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relay_sync_filters: Vec<TransportRelaySyncFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outbound_messages: Vec<TransportRuntimeOutboundMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TransportRuntimePublishRequest {
    request_id: String,
    circle_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    outbound_messages: Vec<TransportRuntimeOutboundMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

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
            sync_since_created_at: None,
            relay_sync_filters: Vec::new(),
            outbound_messages: Vec::new(),
        }
    }

    fn runtime_publish_request() -> TransportRuntimePublishRequest {
        TransportRuntimePublishRequest {
            request_id: "publish:main-circle:4".into(),
            circle_id: "main-circle".into(),
            outbound_messages: vec![outbound_message("alice", "message-1", "event-1")],
        }
    }

    fn outbound_message(
        session_id: &str,
        message_id: &str,
        remote_id: &str,
    ) -> TransportRuntimeOutboundMessage {
        TransportRuntimeOutboundMessage {
            session_id: session_id.into(),
            message_id: message_id.into(),
            remote_id: remote_id.into(),
            signed_nostr_event: SignedNostrEvent {
                event_id: remote_id.into(),
                pubkey: "02b4631d6f1d6659d8e7a0f4d1f56ea74413c5fc11d16f55b3e25a03e353dd1510".into(),
                created_at: 1_735_689_600,
                kind: 1,
                tags: Vec::new(),
                content: "hello from runtime queue".into(),
                signature: "a".repeat(128),
            },
        }
    }

    fn preview_relay_options() -> PreviewRuntimeOptions {
        PreviewRuntimeOptions {
            mode: PreviewRuntimeMode::Relay,
            circle_id: "main-circle".into(),
            relay_url: None,
            session_id: Some("alice".into()),
            use_tor_network: false,
        }
    }

    fn spawn_test_relay_server(
        accepted: bool,
    ) -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        listener
            .set_nonblocking(true)
            .expect("test relay listener should support nonblocking mode");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let (stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("relay should accept one publish connection: {error}"),
                }
            };
            let mut socket = tungstenite::accept(stream).expect("relay websocket handshake");
            let payload = socket.read().expect("relay should read event frame");
            let WebSocketMessage::Text(payload) = payload else {
                panic!("relay expected text event frame");
            };
            let event_id = serde_json::from_str::<serde_json::Value>(&payload)
                .expect("relay should parse event payload")
                .as_array()
                .and_then(|items| items.get(1))
                .and_then(|item| item.get("id"))
                .and_then(|item| item.as_str())
                .expect("relay payload should include event id")
                .to_string();
            let ack = serde_json::json!([
                "OK",
                event_id,
                accepted,
                if accepted {
                    "stored"
                } else {
                    "invalid: blocked by test relay"
                }
            ]);
            socket
                .send(WebSocketMessage::Text(ack.to_string().into()))
                .expect("relay should send ok frame");
            tx.send(payload.to_string())
                .expect("relay payload should reach test receiver");

            let deadline = std::time::Instant::now() + Duration::from_millis(300);
            while std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((sync_stream, _)) => {
                        let mut sync_socket =
                            tungstenite::accept(sync_stream).expect("relay sync handshake");
                        let sync_payload = sync_socket
                            .read()
                            .expect("relay should read sync req frame");
                        let WebSocketMessage::Text(sync_payload) = sync_payload else {
                            panic!("relay expected text sync req frame");
                        };
                        let subscription_id =
                            serde_json::from_str::<serde_json::Value>(&sync_payload)
                                .expect("relay should parse sync req payload")
                                .as_array()
                                .and_then(|items| items.get(1))
                                .and_then(|item| item.as_str())
                                .expect("relay sync req should include subscription id")
                                .to_string();
                        sync_socket
                            .send(WebSocketMessage::Text(
                                serde_json::json!(["EOSE", subscription_id])
                                    .to_string()
                                    .into(),
                            ))
                            .expect("relay should send sync eose frame");
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("relay should accept optional sync connection: {error}"),
                }
            }
        });

        (format!("ws://{}", address), rx, handle)
    }

    fn relay_text_note_event(event_id: &str, content: &str) -> serde_json::Value {
        serde_json::json!({
            "id": event_id,
            "pubkey": "02b4631d6f1d6659d8e7a0f4d1f56ea74413c5fc11d16f55b3e25a03e353dd1510",
            "created_at": 1_735_689_600,
            "kind": 1,
            "tags": [],
            "content": content,
            "sig": "b".repeat(128),
        })
    }

    fn spawn_sync_test_relay_server(
        events: Vec<serde_json::Value>,
    ) -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener
                .accept()
                .expect("relay should accept one sync connection");
            let mut socket = tungstenite::accept(stream).expect("relay websocket handshake");
            let payload = socket.read().expect("relay should read req frame");
            let WebSocketMessage::Text(payload) = payload else {
                panic!("relay expected text req frame");
            };
            let subscription_id = serde_json::from_str::<serde_json::Value>(&payload)
                .expect("relay should parse req payload")
                .as_array()
                .and_then(|items| items.get(1))
                .and_then(|item| item.as_str())
                .expect("relay req should include subscription id")
                .to_string();
            tx.send(payload.to_string())
                .expect("relay req should reach test receiver");
            for event in events {
                let frame = serde_json::json!(["EVENT", subscription_id, event]);
                socket
                    .send(WebSocketMessage::Text(frame.to_string().into()))
                    .expect("relay should send event frame");
            }
            socket
                .send(WebSocketMessage::Text(
                    serde_json::json!(["EOSE", subscription_id])
                        .to_string()
                        .into(),
                ))
                .expect("relay should send eose frame");
        });

        (format!("ws://{}", address), rx, handle)
    }

    #[test]
    fn parse_args_supports_preview_relay_with_session() {
        let behavior = parse_args(vec![
            "preview-relay".to_string(),
            "--relay-url".to_string(),
            "wss://relay.example.com".to_string(),
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
                relay_url: Some("wss://relay.example.com".into()),
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
                signed_nostr_event: None,
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
        let options = preview_relay_options();
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
    fn build_action_output_events_sync_reads_inbound_relay_messages() {
        let (relay_url, req_rx, server_handle) =
            spawn_sync_test_relay_server(vec![relay_text_note_event(
                "relay-event-1",
                "runtime sync imported relay text note",
            )]);
        let mut options = preview_relay_options();
        options.relay_url = Some(relay_url);
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.sync_since_created_at = Some(1_735_689_300);

        let events = build_action_output_events(&options, &request, 1);

        let req_payload = req_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relay should receive req frame");
        assert!(req_payload.contains("\"REQ\""));
        assert!(req_payload.contains("\"kinds\":[1]"));
        assert!(req_payload.contains("\"limit\":20"));
        assert!(req_payload.contains("\"since\":1735689300"));
        server_handle
            .join()
            .expect("relay server should exit cleanly");

        assert_eq!(events.len(), 4);
        let TransportRuntimeOutputEvent::MergeRemoteMessages(payload) = &events[3] else {
            panic!("expected inbound relay merge event");
        };
        assert_eq!(payload.session_id, "alice");
        assert_eq!(payload.messages.len(), 1);
        assert_eq!(payload.messages[0].id, "relay-event-1");
        assert_eq!(
            payload.messages[0].remote_id.as_deref(),
            Some("relay-event-1")
        );
        assert_eq!(
            payload.messages[0].body,
            "runtime sync imported relay text note"
        );
        assert!(matches!(
            payload.messages[0].sync_source,
            Some(MessageSyncSource::Relay)
        ));
        let signed_event = payload.messages[0]
            .signed_nostr_event
            .as_ref()
            .expect("inbound relay message should preserve signed event envelope");
        assert_eq!(signed_event.event_id, "relay-event-1");
    }

    #[test]
    fn build_action_output_events_sync_encodes_relay_sync_filters() {
        let (relay_url, req_rx, server_handle) = spawn_sync_test_relay_server(Vec::new());
        let mut options = preview_relay_options();
        options.relay_url = Some(relay_url);
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.relay_sync_filters = vec![
            TransportRelaySyncFilter {
                authors: vec!["direct-author".into()],
                tagged_pubkeys: Vec::new(),
            },
            TransportRelaySyncFilter {
                authors: vec!["group-author-a".into(), "group-author-b".into()],
                tagged_pubkeys: vec!["group-author-a".into(), "group-author-b".into()],
            },
        ];

        let _ = build_action_output_events(&options, &request, 1);

        let req_payload = req_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relay should receive req frame");
        assert!(req_payload.contains("\"authors\":[\"direct-author\"]"));
        assert!(req_payload.contains("\"authors\":[\"group-author-a\",\"group-author-b\"]"));
        assert!(req_payload.contains("\"#p\":[\"group-author-a\",\"group-author-b\"]"));
        server_handle
            .join()
            .expect("relay server should exit cleanly");
    }

    #[test]
    fn build_action_output_events_maps_outbound_messages_to_delivery_receipts() {
        let options = preview_relay_options();
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.outbound_messages = vec![
            outbound_message("alice", "message-1", "event-1"),
            outbound_message("bob", "message-2", "event-2"),
        ];

        let events = build_action_output_events(&options, &request, 1);

        assert!(matches!(
            &events[3],
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload)
                if payload.session_id == "alice"
                    && payload.receipts.len() == 1
                    && payload.receipts[0].remote_id == "event-1"
                    && payload.receipts[0].message_id.as_deref() == Some("message-1")
                    && matches!(payload.receipts[0].delivery_status, MessageDeliveryStatus::Sent)
        ));
        assert!(matches!(
            &events[4],
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload)
                if payload.session_id == "bob"
                    && payload.receipts.len() == 1
                    && payload.receipts[0].remote_id == "event-2"
                    && payload.receipts[0].message_id.as_deref() == Some("message-2")
        ));
    }

    #[test]
    fn build_publish_output_events_only_emits_outbound_receipts() {
        let options = preview_relay_options();
        let request = runtime_publish_request();

        let events = build_publish_output_events(&options, &request, 4);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload)
                if payload.session_id == "alice"
                    && payload.receipts.len() == 1
                    && payload.receipts[0].remote_id == "event-1"
                    && matches!(payload.receipts[0].delivery_status, MessageDeliveryStatus::Sent)
        ));
    }

    #[test]
    fn build_action_output_events_publishes_outbound_messages_to_relay_socket() {
        let (relay_url, payload_rx, server_handle) = spawn_test_relay_server(true);
        let mut options = preview_relay_options();
        options.relay_url = Some(relay_url);
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.outbound_messages = vec![outbound_message("alice", "message-1", "event-1")];

        let events = build_action_output_events(&options, &request, 1);

        let payload = payload_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relay should receive published event");
        assert!(payload.contains("\"EVENT\""));
        assert!(payload.contains("\"id\":\"event-1\""));
        assert!(payload.contains("\"sig\":\""));
        server_handle
            .join()
            .expect("relay server should exit cleanly");

        assert_eq!(events.len(), 4);
        assert!(matches!(
            &events[3],
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload)
                if payload.session_id == "alice"
                    && payload.receipts.len() == 1
                    && payload.receipts[0].remote_id == "event-1"
                    && matches!(payload.receipts[0].delivery_status, MessageDeliveryStatus::Sent)
        ));
    }

    #[test]
    fn build_action_output_events_marks_outbound_message_failed_when_relay_rejects_event() {
        let (relay_url, payload_rx, server_handle) = spawn_test_relay_server(false);
        let mut options = preview_relay_options();
        options.relay_url = Some(relay_url);
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.outbound_messages = vec![outbound_message("alice", "message-1", "event-1")];

        let events = build_action_output_events(&options, &request, 1);

        let payload = payload_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relay should receive published event");
        assert!(payload.contains("\"id\":\"event-1\""));
        server_handle
            .join()
            .expect("relay server should exit cleanly");

        assert_eq!(events.len(), 5);
        assert!(matches!(
            &events[3],
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload)
                if payload.session_id == "alice"
                    && payload.receipts.len() == 1
                    && payload.receipts[0].remote_id == "event-1"
                    && matches!(payload.receipts[0].delivery_status, MessageDeliveryStatus::Failed)
                    && payload.receipts[0].acked_at.is_none()
        ));
        assert!(matches!(
            &events[4],
            TransportRuntimeOutputEvent::AppendActivity { activity }
                if matches!(activity.kind, TransportActivityKind::Runtime)
                    && matches!(activity.level, TransportActivityLevel::Warn)
                    && activity.title == "Relay publish failed"
                    && activity.detail.contains("blocked by test relay")
        ));
    }

    #[test]
    fn build_inbound_sync_output_events_skips_current_outbound_echoes() {
        let (relay_url, req_rx, server_handle) = spawn_sync_test_relay_server(vec![
            relay_text_note_event("event-1", "echo from current outbound event"),
            relay_text_note_event("relay-event-2", "peer reply after sync"),
        ]);
        let mut options = preview_relay_options();
        options.relay_url = Some(relay_url);
        let mut request = runtime_action_request(TransportCircleAction::Sync);
        request.outbound_messages = vec![outbound_message("alice", "message-1", "event-1")];

        let events = build_inbound_sync_output_events(&options, &request, 9);

        let req_payload = req_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("relay should receive sync req frame");
        assert!(req_payload.contains("\"REQ\""));
        server_handle
            .join()
            .expect("relay server should exit cleanly");

        assert_eq!(events.len(), 1);
        let TransportRuntimeOutputEvent::MergeRemoteMessages(payload) = &events[0] else {
            panic!("expected filtered inbound relay merge");
        };
        assert_eq!(payload.messages.len(), 1);
        assert_eq!(payload.messages[0].id, "relay-event-2");
        assert_eq!(payload.messages[0].body, "peer reply after sync");
    }

    #[test]
    fn parse_relay_subscription_frame_decodes_event_and_eose() {
        let event_payload = serde_json::json!([
            "EVENT",
            "preview-sync-1",
            relay_text_note_event("relay-event-1", "hello from relay"),
        ])
        .to_string();

        let frame = parse_relay_subscription_frame(&event_payload, "preview-sync-1")
            .expect("event payload should parse")
            .expect("expected event frame");
        let RelaySubscriptionFrame::Event(event) = frame else {
            panic!("expected event frame");
        };
        assert_eq!(event.id, "relay-event-1");
        assert_eq!(event.kind, 1);
        assert_eq!(event.content, "hello from relay");

        let eose_payload = serde_json::json!(["EOSE", "preview-sync-1"]).to_string();
        let frame = parse_relay_subscription_frame(&eose_payload, "preview-sync-1")
            .expect("eose payload should parse")
            .expect("expected eose frame");
        assert!(matches!(frame, RelaySubscriptionFrame::EndOfStoredEvents));
    }

    #[test]
    fn build_action_output_events_maps_discover_peers_to_runtime_message() {
        let options = preview_relay_options();
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
        let options = preview_relay_options();
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

        let TransportRuntimeInputEvent::ApplyCircleAction(request) = event else {
            panic!("expected apply circle action event");
        };
        assert_eq!(
            request,
            runtime_action_request(TransportCircleAction::SyncSessions)
        );
    }

    #[test]
    fn runtime_publish_input_event_deserializes_matching_app_contract() {
        let event = serde_json::from_str::<TransportRuntimeInputEvent>(
            r#"{"kind":"publishOutboundMessages","payload":{"requestId":"publish:main-circle:4","circleId":"main-circle","outboundMessages":[{"sessionId":"alice","messageId":"message-1","remoteId":"event-1","signedNostrEvent":{"eventId":"event-1","pubkey":"02b4631d6f1d6659d8e7a0f4d1f56ea74413c5fc11d16f55b3e25a03e353dd1510","createdAt":1735689600,"tags":[],"kind":1,"content":"hello from runtime queue","signature":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]}}"#,
        )
        .expect("runtime input event should deserialize");

        let TransportRuntimeInputEvent::PublishOutboundMessages(request) = event else {
            panic!("expected publish outbound messages event");
        };
        assert_eq!(request, runtime_publish_request());
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

        std::fs::write(
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
        let TransportRuntimeInputEvent::ApplyCircleAction(request) = &events[0] else {
            panic!("expected apply circle action event");
        };
        assert_eq!(
            request,
            &runtime_action_request(TransportCircleAction::SyncSessions)
        );
        assert!(offset > 0);

        let _ = std::fs::remove_file(queue_path);
    }
}
