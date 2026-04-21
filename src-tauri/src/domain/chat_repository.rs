use crate::domain::chat::{
    ChatDomainSeed, CircleItem, ContactItem, GroupProfile, MessageAuthor, MessageDeliveryStatus,
    MessageItem, MessageKind, MessageSyncSource, RemoteDeliveryReceipt, SessionItem,
};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ChatUpsert<T> {
    pub item: T,
    pub move_to_top: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ChatDomainChangeSet {
    pub circles_upsert: Vec<ChatUpsert<CircleItem>>,
    pub circle_ids_to_delete: Vec<String>,
    pub contacts_upsert: Vec<ChatUpsert<ContactItem>>,
    pub sessions_upsert: Vec<ChatUpsert<SessionItem>>,
    pub session_ids_to_delete: Vec<String>,
    pub groups_upsert: Vec<ChatUpsert<GroupProfile>>,
    pub messages_upsert: Vec<(String, MessageItem)>,
    pub messages_append: Vec<(String, MessageItem)>,
    pub messages_replace: Vec<(String, Vec<MessageItem>)>,
}

pub trait ChatRepository {
    fn load_domain_seed(&self) -> Result<ChatDomainSeed, String>;
    fn save_domain_seed(&self, seed: ChatDomainSeed) -> Result<(), String>;

    fn apply_change_set(&self, change_set: ChatDomainChangeSet) -> Result<(), String> {
        let mut seed = self.load_domain_seed()?;
        apply_change_set_to_seed(&mut seed, change_set);
        self.save_domain_seed(seed)
    }
}

pub fn apply_change_set_to_seed(seed: &mut ChatDomainSeed, change_set: ChatDomainChangeSet) {
    for circle_id in change_set.circle_ids_to_delete {
        let removed_session_ids = seed
            .sessions
            .iter()
            .filter(|session| session.circle_id == circle_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();

        seed.circles.retain(|circle| circle.id != circle_id);
        seed.sessions
            .retain(|session| session.circle_id != circle_id);
        seed.groups
            .retain(|group| !removed_session_ids.contains(&group.session_id));
        seed.message_store
            .retain(|session_id, _| !removed_session_ids.contains(session_id));
    }

    for session_id in change_set.session_ids_to_delete {
        seed.sessions.retain(|session| session.id != session_id);
        seed.groups.retain(|group| group.session_id != session_id);
        seed.message_store.remove(&session_id);
    }

    for upsert in change_set.circles_upsert {
        upsert_circle(seed, upsert);
    }

    for upsert in change_set.contacts_upsert {
        upsert_contact(seed, upsert);
    }

    for upsert in change_set.sessions_upsert {
        upsert_session(seed, upsert);
    }

    for upsert in change_set.groups_upsert {
        upsert_group(seed, upsert);
    }

    for (session_id, messages) in change_set.messages_replace {
        seed.message_store.insert(session_id, messages);
    }

    for (session_id, message) in change_set.messages_upsert {
        upsert_message(seed, &session_id, message);
    }

    for (session_id, message) in change_set.messages_append {
        seed.message_store
            .entry(session_id)
            .or_default()
            .push(message);
    }
}

pub fn merge_remote_messages_into_seed(
    seed: &mut ChatDomainSeed,
    session_id: &str,
    messages: Vec<MessageItem>,
) -> Result<(), String> {
    let change_set = build_remote_message_merge_change_set(seed, session_id, messages)?;
    apply_change_set_to_seed(seed, change_set);
    Ok(())
}

pub fn build_remote_message_merge_change_set(
    seed: &ChatDomainSeed,
    session_id: &str,
    messages: Vec<MessageItem>,
) -> Result<ChatDomainChangeSet, String> {
    if messages.is_empty() {
        return Ok(ChatDomainChangeSet::default());
    }

    let mut session = seed
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    let mut seen_message_ids = seed
        .message_store
        .values()
        .flat_map(|bucket| bucket.iter().map(|message| message.id.clone()))
        .collect::<HashSet<_>>();
    let mut seen_remote_ids = seed
        .message_store
        .values()
        .flat_map(|bucket| {
            bucket
                .iter()
                .filter_map(|message| message.remote_id.clone())
        })
        .collect::<HashSet<_>>();
    let mut new_peer_message_count = 0u32;
    let mut latest_remote_activity_message = None;
    let mut normalized_messages = Vec::with_capacity(messages.len());

    for message in messages {
        let mut normalized_message = normalize_remote_message(message);
        hydrate_remote_message_reply_to(seed, &normalized_messages, &mut normalized_message);
        let is_known_id = seen_message_ids.contains(&normalized_message.id);
        let is_known_remote_id = normalized_message
            .remote_id
            .as_ref()
            .is_some_and(|remote_id| seen_remote_ids.contains(remote_id));
        let is_new_message = !is_known_id && !is_known_remote_id;

        if is_new_message && matches!(normalized_message.author, MessageAuthor::Peer) {
            new_peer_message_count = new_peer_message_count.saturating_add(1);
        }
        if is_new_message && message_counts_as_remote_session_activity(&normalized_message) {
            if should_replace_latest_remote_activity_message(
                latest_remote_activity_message.as_ref(),
                &normalized_message,
            ) {
                latest_remote_activity_message = Some(normalized_message.clone());
            }
        }

        seen_message_ids.insert(normalized_message.id.clone());
        if let Some(remote_id) = normalized_message.remote_id.clone() {
            seen_remote_ids.insert(remote_id);
        }
        normalized_messages.push(normalized_message);
    }

    let has_peer_message = normalized_messages
        .iter()
        .any(|message| matches!(message.author, MessageAuthor::Peer));
    let mut change_set = ChatDomainChangeSet {
        messages_upsert: normalized_messages
            .into_iter()
            .map(|message| (session_id.to_string(), message))
            .collect(),
        ..Default::default()
    };

    if let Some(latest_message) = latest_remote_activity_message {
        session.subtitle = message_preview_text(&latest_message);
        session.time = latest_message.time.clone();
        session.unread_count = match session.unread_count {
            Some(current) => Some(current.saturating_add(new_peer_message_count)),
            None if new_peer_message_count > 0 => Some(new_peer_message_count),
            None => None,
        };
        change_set.sessions_upsert.push(ChatUpsert {
            item: session.clone(),
            move_to_top: true,
        });
    }

    if has_peer_message {
        if let Some(contact_id) = session.contact_id.clone() {
            if let Some(contact) = seed
                .contacts
                .iter()
                .find(|contact| contact.id == contact_id)
                .cloned()
            {
                change_set.contacts_upsert.push(ChatUpsert {
                    item: ContactItem {
                        online: Some(true),
                        ..contact
                    },
                    move_to_top: false,
                });
            }
        }
    }

    Ok(change_set)
}

fn message_counts_as_remote_session_activity(message: &MessageItem) -> bool {
    !matches!(message.author, MessageAuthor::Me)
        || matches!(message.sync_source, Some(MessageSyncSource::Relay))
}

fn should_replace_latest_remote_activity_message(
    current: Option<&MessageItem>,
    candidate: &MessageItem,
) -> bool {
    let Some(current) = current else {
        return true;
    };

    match (
        current
            .signed_nostr_event
            .as_ref()
            .map(|event| event.created_at),
        candidate
            .signed_nostr_event
            .as_ref()
            .map(|event| event.created_at),
    ) {
        (Some(current_created_at), Some(candidate_created_at)) => {
            candidate_created_at >= current_created_at
        }
        _ => true,
    }
}

pub fn build_remote_delivery_receipt_change_set(
    seed: &ChatDomainSeed,
    session_id: &str,
    receipts: Vec<RemoteDeliveryReceipt>,
) -> Result<ChatDomainChangeSet, String> {
    if receipts.is_empty() {
        return Ok(ChatDomainChangeSet::default());
    }

    if !seed.sessions.iter().any(|session| session.id == session_id) {
        return Err(format!("session not found: {session_id}"));
    }

    let Some(messages) = seed.message_store.get(session_id) else {
        return Ok(ChatDomainChangeSet::default());
    };

    let mut change_set = ChatDomainChangeSet::default();

    for receipt in receipts {
        let Some(message) = find_message_for_receipt(messages, &receipt).cloned() else {
            continue;
        };

        if !matches!(message.author, MessageAuthor::Me) {
            continue;
        }

        change_set.messages_upsert.push((
            session_id.to_string(),
            merge_delivery_receipt(message, receipt),
        ));
    }

    Ok(change_set)
}

fn upsert_circle(seed: &mut ChatDomainSeed, upsert: ChatUpsert<CircleItem>) {
    upsert_with_position(&mut seed.circles, upsert, |left, right| left.id == right.id);
}

fn upsert_contact(seed: &mut ChatDomainSeed, upsert: ChatUpsert<ContactItem>) {
    upsert_with_position(&mut seed.contacts, upsert, |left, right| {
        left.id == right.id
    });
}

fn upsert_session(seed: &mut ChatDomainSeed, upsert: ChatUpsert<SessionItem>) {
    upsert_with_position(&mut seed.sessions, upsert, |left, right| {
        left.id == right.id
    });
}

fn upsert_group(seed: &mut ChatDomainSeed, upsert: ChatUpsert<GroupProfile>) {
    upsert_with_position(&mut seed.groups, upsert, |left, right| {
        left.session_id == right.session_id
    });
}

fn upsert_message(seed: &mut ChatDomainSeed, session_id: &str, message: MessageItem) {
    let mut existing_location = None;

    for (bucket_session_id, messages) in &seed.message_store {
        if let Some(index) = messages
            .iter()
            .position(|item| same_message_identity(item, &message))
        {
            existing_location = Some((bucket_session_id.clone(), index, messages[index].clone()));
            break;
        }
    }

    match existing_location {
        Some((existing_session_id, index, existing_message))
            if existing_session_id == session_id =>
        {
            let merged_message = merge_message_records(existing_message, message);
            if let Some(messages) = seed.message_store.get_mut(session_id) {
                messages[index] = merged_message;
            }
        }
        Some((existing_session_id, index, existing_message)) => {
            let merged_message = merge_message_records(existing_message, message);
            if let Some(messages) = seed.message_store.get_mut(&existing_session_id) {
                messages.remove(index);
            }
            seed.message_store
                .entry(session_id.to_string())
                .or_default()
                .push(merged_message);
        }
        None => {
            seed.message_store
                .entry(session_id.to_string())
                .or_default()
                .push(message);
        }
    }
}

fn same_message_identity(existing: &MessageItem, incoming: &MessageItem) -> bool {
    if existing.id == incoming.id {
        return true;
    }

    matches!(
        (existing.remote_id.as_deref(), incoming.remote_id.as_deref()),
        (Some(existing_remote_id), Some(incoming_remote_id)) if existing_remote_id == incoming_remote_id
    )
}

pub(crate) fn merge_message_records(existing: MessageItem, incoming: MessageItem) -> MessageItem {
    let preserve_existing_id = existing.id != incoming.id
        && existing.remote_id.is_some()
        && existing.remote_id == incoming.remote_id;
    let preserve_existing_author = preserve_existing_id
        && !matches!(&existing.author, MessageAuthor::Peer)
        && matches!(&incoming.author, MessageAuthor::Peer);
    let preserve_existing_sync_source = preserve_existing_id
        && matches!(
            existing.sync_source.as_ref(),
            Some(MessageSyncSource::Local)
        )
        && matches!(
            incoming.sync_source.as_ref(),
            Some(MessageSyncSource::Relay)
        );
    let merged_meta =
        merge_message_meta(&incoming.kind, existing.meta.clone(), incoming.meta.clone());

    MessageItem {
        id: if preserve_existing_id {
            existing.id
        } else {
            incoming.id
        },
        kind: incoming.kind,
        author: if preserve_existing_author {
            existing.author
        } else {
            incoming.author
        },
        body: if incoming.body.is_empty() {
            existing.body
        } else {
            incoming.body
        },
        time: if incoming.time.is_empty() {
            existing.time
        } else {
            incoming.time
        },
        meta: merged_meta,
        delivery_status: incoming.delivery_status.or(existing.delivery_status),
        remote_id: incoming.remote_id.or(existing.remote_id),
        sync_source: if preserve_existing_sync_source {
            existing.sync_source.or(incoming.sync_source)
        } else {
            incoming.sync_source.or(existing.sync_source)
        },
        acked_at: incoming.acked_at.or(existing.acked_at),
        signed_nostr_event: incoming.signed_nostr_event.or(existing.signed_nostr_event),
        reply_to: incoming.reply_to.or(existing.reply_to),
    }
}

#[derive(Debug, Clone, Default)]
struct NormalizedMediaMeta {
    label: String,
    preview_data_url: Option<String>,
    local_path: Option<String>,
    remote_url: Option<String>,
}

pub(crate) fn merge_message_meta(
    kind: &MessageKind,
    existing: Option<String>,
    incoming: Option<String>,
) -> Option<String> {
    match kind {
        MessageKind::File | MessageKind::Image | MessageKind::Video => {
            merge_structured_media_message_meta(kind, existing.as_deref(), incoming.as_deref())
                .or(incoming)
                .or(existing)
        }
        _ => incoming.or(existing),
    }
}

fn merge_structured_media_message_meta(
    kind: &MessageKind,
    existing: Option<&str>,
    incoming: Option<&str>,
) -> Option<String> {
    let existing_meta = existing.and_then(|value| decode_structured_media_meta(kind, value));
    let incoming_meta = incoming.and_then(|value| decode_structured_media_meta(kind, value));
    if existing_meta.is_none() && incoming_meta.is_none() {
        return None;
    }

    let merged = NormalizedMediaMeta {
        label: incoming_meta
            .as_ref()
            .map(|meta| meta.label.clone())
            .or_else(|| existing_meta.as_ref().map(|meta| meta.label.clone()))
            .unwrap_or_default(),
        preview_data_url: incoming_meta
            .as_ref()
            .and_then(|meta| meta.preview_data_url.clone())
            .or_else(|| {
                existing_meta
                    .as_ref()
                    .and_then(|meta| meta.preview_data_url.clone())
            }),
        local_path: incoming_meta
            .as_ref()
            .and_then(|meta| meta.local_path.clone())
            .or_else(|| {
                existing_meta
                    .as_ref()
                    .and_then(|meta| meta.local_path.clone())
            }),
        remote_url: incoming_meta
            .as_ref()
            .and_then(|meta| meta.remote_url.clone())
            .or_else(|| {
                existing_meta
                    .as_ref()
                    .and_then(|meta| meta.remote_url.clone())
            }),
    };

    encode_structured_media_meta(kind, merged)
}

pub(crate) fn message_media_local_path(message: &MessageItem) -> Option<String> {
    decode_structured_media_meta(&message.kind, message.meta.as_deref()?)?.local_path
}

pub(crate) fn message_media_remote_url(message: &MessageItem) -> Option<String> {
    decode_structured_media_meta(&message.kind, message.meta.as_deref()?)?.remote_url
}

pub(crate) fn message_media_label(message: &MessageItem) -> Option<String> {
    normalized_message_media_meta(message).map(|meta| meta.label)
}

pub(crate) fn message_media_meta_with_local_path(
    message: &MessageItem,
    local_path: String,
) -> Option<String> {
    let mut meta = normalized_message_media_meta(message)?;
    meta.local_path = Some(local_path);
    encode_structured_media_meta(&message.kind, meta)
}

pub(crate) fn message_media_meta_with_remote_url(
    message: &MessageItem,
    remote_url: String,
) -> Option<String> {
    let mut meta = normalized_message_media_meta(message)?;
    meta.remote_url = Some(remote_url);
    encode_structured_media_meta(&message.kind, meta)
}

fn normalized_message_media_meta(message: &MessageItem) -> Option<NormalizedMediaMeta> {
    decode_structured_media_meta(&message.kind, message.meta.as_deref()?).or_else(|| {
        if !matches!(message.kind, MessageKind::File) {
            return None;
        }

        let label = message
            .meta
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| Some(message.body.trim()).filter(|value| !value.is_empty()))?;

        Some(NormalizedMediaMeta {
            label: label.to_string(),
            ..Default::default()
        })
    })
}

fn decode_structured_media_meta(kind: &MessageKind, value: &str) -> Option<NormalizedMediaMeta> {
    let parsed = serde_json::from_str::<JsonValue>(value).ok()?;
    let version = parsed.get("version")?.as_u64()?;
    let label = json_non_empty_string(&parsed, "label")?;

    match kind {
        MessageKind::File => {
            if !matches!(version, 1 | 2) {
                return None;
            }

            let local_path = json_non_empty_string(&parsed, "localPath");
            let remote_url = json_non_empty_string(&parsed, "remoteUrl");
            if local_path.is_none() && remote_url.is_none() {
                return None;
            }

            Some(NormalizedMediaMeta {
                label,
                local_path,
                remote_url,
                ..Default::default()
            })
        }
        MessageKind::Image | MessageKind::Video => match version {
            1 => Some(NormalizedMediaMeta {
                label,
                preview_data_url: json_non_empty_string(&parsed, "previewDataUrl"),
                ..Default::default()
            })
            .filter(|meta| meta.preview_data_url.is_some()),
            2 => Some(NormalizedMediaMeta {
                label,
                local_path: json_non_empty_string(&parsed, "localPath"),
                ..Default::default()
            })
            .filter(|meta| meta.local_path.is_some()),
            3 => {
                let local_path = json_non_empty_string(&parsed, "localPath");
                let remote_url = json_non_empty_string(&parsed, "remoteUrl");
                if local_path.is_none() && remote_url.is_none() {
                    return None;
                }

                Some(NormalizedMediaMeta {
                    label,
                    local_path,
                    remote_url,
                    ..Default::default()
                })
            }
            _ => None,
        },
        _ => None,
    }
}

fn encode_structured_media_meta(kind: &MessageKind, meta: NormalizedMediaMeta) -> Option<String> {
    if meta.label.trim().is_empty() {
        return None;
    }

    match kind {
        MessageKind::File => {
            let version = if meta.remote_url.is_some() || meta.local_path.is_none() {
                2
            } else {
                1
            };
            let mut payload = JsonMap::new();
            payload.insert("version".into(), JsonValue::from(version));
            payload.insert("label".into(), JsonValue::from(meta.label));
            if let Some(local_path) = meta.local_path {
                payload.insert("localPath".into(), JsonValue::from(local_path));
            }
            if let Some(remote_url) = meta.remote_url {
                payload.insert("remoteUrl".into(), JsonValue::from(remote_url));
            }
            Some(JsonValue::Object(payload).to_string())
        }
        MessageKind::Image | MessageKind::Video => {
            if meta.local_path.is_some() || meta.remote_url.is_some() {
                let version = if meta.remote_url.is_some() || meta.local_path.is_none() {
                    3
                } else {
                    2
                };
                let mut payload = JsonMap::new();
                payload.insert("version".into(), JsonValue::from(version));
                payload.insert("label".into(), JsonValue::from(meta.label));
                if let Some(local_path) = meta.local_path {
                    payload.insert("localPath".into(), JsonValue::from(local_path));
                }
                if let Some(remote_url) = meta.remote_url {
                    payload.insert("remoteUrl".into(), JsonValue::from(remote_url));
                }
                return Some(JsonValue::Object(payload).to_string());
            }

            meta.preview_data_url.map(|preview_data_url| {
                let mut payload = JsonMap::new();
                payload.insert("version".into(), JsonValue::from(1));
                payload.insert("label".into(), JsonValue::from(meta.label));
                payload.insert("previewDataUrl".into(), JsonValue::from(preview_data_url));
                JsonValue::Object(payload).to_string()
            })
        }
        _ => None,
    }
}

fn json_non_empty_string(parsed: &JsonValue, key: &str) -> Option<String> {
    parsed
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_remote_message(mut message: MessageItem) -> MessageItem {
    if message.sync_source.is_none() {
        message.sync_source = Some(match message.author {
            MessageAuthor::Me => MessageSyncSource::Local,
            MessageAuthor::Peer => MessageSyncSource::Relay,
            MessageAuthor::System => MessageSyncSource::System,
        });
    }

    if matches!(message.author, MessageAuthor::Peer) && message.remote_id.is_none() {
        message.remote_id = Some(format!("relay:{}", message.id));
    }

    message
}

fn message_preview_text(message: &MessageItem) -> String {
    match message.kind {
        MessageKind::System | MessageKind::Text => message.body.clone(),
        MessageKind::Image => format!("Shared image: {}", message.body),
        MessageKind::Video => format!("Shared video: {}", message.body),
        MessageKind::File => format!("Shared file: {}", message.body),
        MessageKind::Audio => {
            let label = message.meta.as_deref().unwrap_or("Voice note");
            format!("Audio: {label}")
        }
    }
}

fn message_reply_preview_text(message: &MessageItem) -> String {
    let preview = message_preview_text(message);
    let trimmed = preview.trim();
    if trimmed.is_empty() {
        return "Empty message".into();
    }

    const MAX_REPLY_SNIPPET_CHARS: usize = 96;
    let snippet = trimmed
        .chars()
        .take(MAX_REPLY_SNIPPET_CHARS)
        .collect::<String>();
    if trimmed.chars().count() > MAX_REPLY_SNIPPET_CHARS {
        format!("{snippet}...")
    } else {
        snippet
    }
}

fn default_reply_author_label(author: &MessageAuthor) -> &'static str {
    match author {
        MessageAuthor::Me => "You",
        MessageAuthor::Peer => "Peer",
        MessageAuthor::System => "System",
    }
}

pub(crate) fn build_message_reply_preview(
    message: &MessageItem,
) -> crate::domain::chat::MessageReplyPreview {
    crate::domain::chat::MessageReplyPreview {
        message_id: message.id.clone(),
        remote_id: message.remote_id.clone().or_else(|| {
            message
                .signed_nostr_event
                .as_ref()
                .map(|event| event.event_id.clone())
        }),
        author: message.author.clone(),
        author_label: default_reply_author_label(&message.author).into(),
        kind: message.kind.clone(),
        snippet: message_reply_preview_text(message),
    }
}

fn unresolved_message_reply_preview(
    reply_reference_id: &str,
) -> crate::domain::chat::MessageReplyPreview {
    crate::domain::chat::MessageReplyPreview {
        message_id: reply_reference_id.into(),
        remote_id: Some(reply_reference_id.into()),
        author: MessageAuthor::System,
        author_label: "Quoted message".into(),
        kind: MessageKind::Text,
        snippet: "Referenced message".into(),
    }
}

fn reply_reference_id_from_tags(message: &MessageItem) -> Option<String> {
    message.signed_nostr_event.as_ref().and_then(|event| {
        event
            .tags
            .iter()
            .find_map(|tag| match tag.first().map(String::as_str) {
                Some("e") => tag.get(1).cloned(),
                _ => None,
            })
    })
}

fn hydrate_remote_message_reply_to(
    seed: &ChatDomainSeed,
    pending_messages: &[MessageItem],
    message: &mut MessageItem,
) {
    if message.reply_to.is_some() {
        return;
    }

    let Some(reply_reference_id) = reply_reference_id_from_tags(message) else {
        return;
    };

    if let Some(existing_message) = pending_messages
        .iter()
        .find(|candidate| message_matches_reply_reference(candidate, &reply_reference_id))
        .or_else(|| {
            seed.message_store
                .values()
                .flat_map(|messages| messages.iter())
                .find(|candidate| message_matches_reply_reference(candidate, &reply_reference_id))
        })
    {
        message.reply_to = Some(build_message_reply_preview(existing_message));
        return;
    }

    message.reply_to = Some(unresolved_message_reply_preview(&reply_reference_id));
}

fn message_matches_reply_reference(message: &MessageItem, reply_reference_id: &str) -> bool {
    message.id == reply_reference_id
        || message.remote_id.as_deref() == Some(reply_reference_id)
        || message
            .signed_nostr_event
            .as_ref()
            .is_some_and(|event| event.event_id == reply_reference_id)
}

fn merge_delivery_receipt(mut message: MessageItem, receipt: RemoteDeliveryReceipt) -> MessageItem {
    message.delivery_status = Some(receipt.delivery_status);
    if message.remote_id.is_none() && !receipt.remote_id.is_empty() {
        message.remote_id = Some(receipt.remote_id);
    }
    if message.sync_source.is_none() {
        message.sync_source = Some(MessageSyncSource::Local);
    }
    if let Some(acked_at) = receipt.acked_at {
        message.acked_at = Some(acked_at);
    } else if matches!(message.delivery_status, Some(MessageDeliveryStatus::Sent))
        && message.acked_at.is_none()
    {
        message.acked_at = Some(message.time.clone());
    }

    message
}

fn find_message_for_receipt<'a>(
    messages: &'a [MessageItem],
    receipt: &RemoteDeliveryReceipt,
) -> Option<&'a MessageItem> {
    if !receipt.remote_id.is_empty() {
        if let Some(message) = messages
            .iter()
            .find(|message| message.remote_id.as_deref() == Some(receipt.remote_id.as_str()))
        {
            return Some(message);
        }
    }

    receipt
        .message_id
        .as_deref()
        .and_then(|message_id| messages.iter().find(|message| message.id == message_id))
}

fn upsert_with_position<T: Clone>(
    items: &mut Vec<T>,
    upsert: ChatUpsert<T>,
    matches: impl Fn(&T, &T) -> bool,
) {
    let existing_index = items.iter().position(|item| matches(item, &upsert.item));

    match existing_index {
        Some(index) if upsert.move_to_top => {
            items.remove(index);
            items.insert(0, upsert.item);
        }
        Some(index) => {
            items[index] = upsert.item;
        }
        None if upsert.move_to_top => {
            items.insert(0, upsert.item);
        }
        None => {
            items.push(upsert.item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as JsonValue;

    fn test_message(kind: MessageKind, meta: Option<&str>) -> MessageItem {
        MessageItem {
            id: "message-1".into(),
            kind,
            author: MessageAuthor::Me,
            body: "media-body".into(),
            time: "09:41".into(),
            meta: meta.map(str::to_string),
            delivery_status: None,
            remote_id: Some("relay:message-1".into()),
            sync_source: Some(MessageSyncSource::Local),
            acked_at: None,
            signed_nostr_event: None,
            reply_to: None,
        }
    }

    fn assert_json_meta_eq(actual: Option<&str>, expected: &str) {
        let actual_value = actual.map(|value| {
            serde_json::from_str::<JsonValue>(value).expect("actual json should parse")
        });
        let expected_value =
            serde_json::from_str::<JsonValue>(expected).expect("expected json should parse");
        assert_eq!(actual_value, Some(expected_value));
    }

    #[test]
    fn merge_message_records_keeps_local_image_path_and_adds_remote_url() {
        let existing = test_message(
            MessageKind::Image,
            Some(
                r#"{"version":2,"label":"PNG · 84 KB","localPath":"/tmp/chat-media/images/local.png"}"#,
            ),
        );
        let incoming = MessageItem {
            author: MessageAuthor::Peer,
            sync_source: Some(MessageSyncSource::Relay),
            meta: Some(
                r#"{"version":3,"label":"PNG · CDN copy","remoteUrl":"https://cdn.example.test/chat-media/local.png"}"#
                    .into(),
            ),
            ..test_message(MessageKind::Image, None)
        };

        let merged = merge_message_records(existing, incoming);

        assert_json_meta_eq(
            merged.meta.as_deref(),
            r#"{"version":3,"label":"PNG · CDN copy","localPath":"/tmp/chat-media/images/local.png","remoteUrl":"https://cdn.example.test/chat-media/local.png"}"#,
        );
    }

    #[test]
    fn merge_message_records_keeps_remote_video_url_when_local_cache_arrives() {
        let existing = test_message(
            MessageKind::Video,
            Some(
                r#"{"version":3,"label":"MP4 · Remote","remoteUrl":"https://cdn.example.test/chat-media/clip.mp4"}"#,
            ),
        );
        let incoming = MessageItem {
            meta: Some(
                r#"{"version":2,"label":"MP4 · Downloaded","localPath":"/tmp/chat-media/videos/clip.mp4"}"#
                    .into(),
            ),
            ..test_message(MessageKind::Video, None)
        };

        let merged = merge_message_records(existing, incoming);

        assert_json_meta_eq(
            merged.meta.as_deref(),
            r#"{"version":3,"label":"MP4 · Downloaded","localPath":"/tmp/chat-media/videos/clip.mp4","remoteUrl":"https://cdn.example.test/chat-media/clip.mp4"}"#,
        );
    }

    #[test]
    fn merge_message_records_promotes_file_meta_to_remote_capable_shape() {
        let existing = test_message(
            MessageKind::File,
            Some(
                r#"{"version":1,"label":"PDF · 2.4 MB","localPath":"/tmp/chat-media/files/contract.pdf"}"#,
            ),
        );
        let incoming = MessageItem {
            author: MessageAuthor::Peer,
            sync_source: Some(MessageSyncSource::Relay),
            meta: Some(
                r#"{"version":2,"label":"PDF · shared","remoteUrl":"https://cdn.example.test/chat-media/contract.pdf"}"#
                    .into(),
            ),
            ..test_message(MessageKind::File, None)
        };

        let merged = merge_message_records(existing, incoming);

        assert_json_meta_eq(
            merged.meta.as_deref(),
            r#"{"version":2,"label":"PDF · shared","localPath":"/tmp/chat-media/files/contract.pdf","remoteUrl":"https://cdn.example.test/chat-media/contract.pdf"}"#,
        );
    }

    #[test]
    fn message_media_meta_with_remote_url_promotes_plain_file_meta() {
        let message = MessageItem {
            body: "roadmap.pdf".into(),
            meta: Some("PDF · 2.4 MB".into()),
            ..test_message(MessageKind::File, None)
        };

        let updated = message_media_meta_with_remote_url(
            &message,
            "https://cdn.example.test/chat-media/roadmap.pdf".into(),
        )
        .expect("plain file meta should promote to remote-capable shape");

        assert_json_meta_eq(
            Some(updated.as_str()),
            r#"{"version":2,"label":"PDF · 2.4 MB","remoteUrl":"https://cdn.example.test/chat-media/roadmap.pdf"}"#,
        );
    }

    #[test]
    fn message_media_meta_with_remote_url_keeps_existing_local_image_path() {
        let message = MessageItem {
            body: "roadmap.png".into(),
            meta: Some(
                r#"{"version":2,"label":"PNG · 1280 x 720","localPath":"/tmp/chat-media/images/roadmap.png"}"#
                    .into(),
            ),
            ..test_message(MessageKind::Image, None)
        };

        let updated = message_media_meta_with_remote_url(
            &message,
            "https://cdn.example.test/chat-media/roadmap.png".into(),
        )
        .expect("local image meta should keep local path while adding remote url");

        assert_json_meta_eq(
            Some(updated.as_str()),
            r#"{"version":3,"label":"PNG · 1280 x 720","localPath":"/tmp/chat-media/images/roadmap.png","remoteUrl":"https://cdn.example.test/chat-media/roadmap.png"}"#,
        );
    }
}
