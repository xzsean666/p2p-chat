use crate::domain::chat::{
    ChatDomainSeed, CircleItem, ContactItem, GroupProfile, MessageAuthor, MessageDeliveryStatus,
    MessageItem, MessageKind, MessageSyncSource, RemoteDeliveryReceipt, SessionItem,
};
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
    let mut has_new_non_local_message = false;
    let mut normalized_messages = Vec::with_capacity(messages.len());

    for message in messages {
        let normalized_message = normalize_remote_message(message);
        let is_known_id = seen_message_ids.contains(&normalized_message.id);
        let is_known_remote_id = normalized_message
            .remote_id
            .as_ref()
            .is_some_and(|remote_id| seen_remote_ids.contains(remote_id));

        if !is_known_id
            && !is_known_remote_id
            && matches!(normalized_message.author, MessageAuthor::Peer)
        {
            new_peer_message_count = new_peer_message_count.saturating_add(1);
        }
        if !is_known_id
            && !is_known_remote_id
            && !matches!(normalized_message.author, MessageAuthor::Me)
        {
            has_new_non_local_message = true;
        }

        seen_message_ids.insert(normalized_message.id.clone());
        if let Some(remote_id) = normalized_message.remote_id.clone() {
            seen_remote_ids.insert(remote_id);
        }
        normalized_messages.push(normalized_message);
    }

    let latest_message = normalized_messages
        .last()
        .cloned()
        .ok_or_else(|| "missing latest remote message".to_string())?;
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

    if has_new_non_local_message {
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

    MessageItem {
        id: if preserve_existing_id {
            existing.id
        } else {
            incoming.id
        },
        kind: incoming.kind,
        author: incoming.author,
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
        meta: incoming.meta.or(existing.meta),
        delivery_status: incoming.delivery_status.or(existing.delivery_status),
        remote_id: incoming.remote_id.or(existing.remote_id),
        sync_source: incoming.sync_source.or(existing.sync_source),
        acked_at: incoming.acked_at.or(existing.acked_at),
    }
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
        MessageKind::File => format!("Shared file: {}", message.body),
        MessageKind::Audio => {
            let label = message.meta.as_deref().unwrap_or("Voice note");
            format!("Audio: {label}")
        }
    }
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
