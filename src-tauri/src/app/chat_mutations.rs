use crate::domain::chat::{
    AddCircleInput, AddCircleResult, ChatDomainSeed, ChatSessionAction, CircleCreateMode,
    CircleItem, CircleStatus, CircleType, MessageAuthor, MessageItem, MessageKind,
    SendMessageInput, SessionActionInput, SessionItem, SessionKind, StartConversationInput,
    StartConversationResult, UpdateCircleInput,
};
use crate::domain::chat_repository::ChatRepository;
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn send_message(
    app_handle: &tauri::AppHandle,
    input: SendMessageInput,
) -> Result<ChatDomainSeed, String> {
    let content = input.body.trim();
    if content.is_empty() {
        return Err("message body is empty".into());
    }

    let mut seed = load_domain_seed(app_handle)?;
    let target_index = seed
        .sessions
        .iter()
        .position(|session| session.id == input.session_id)
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;

    seed.message_store
        .entry(input.session_id.clone())
        .or_insert_with(Vec::new)
        .push(MessageItem {
            id: unique_local_id("message"),
            kind: MessageKind::Text,
            author: MessageAuthor::Me,
            body: content.to_string(),
            time: "now".into(),
            meta: None,
        });

    let mut updated_session = seed.sessions.remove(target_index);
    updated_session.subtitle = content.to_string();
    updated_session.time = "now".into();
    updated_session.draft = None;
    seed.sessions.insert(0, updated_session);

    save_domain_seed(app_handle, seed)
}

pub fn start_conversation(
    app_handle: &tauri::AppHandle,
    input: StartConversationInput,
) -> Result<StartConversationResult, String> {
    let mut seed = load_domain_seed(app_handle)?;
    if !seed
        .circles
        .iter()
        .any(|circle| circle.id == input.circle_id)
    {
        return Err(format!("circle not found: {}", input.circle_id));
    }

    let contact = seed
        .contacts
        .iter()
        .find(|item| item.id == input.contact_id)
        .cloned()
        .ok_or_else(|| format!("contact not found: {}", input.contact_id))?;

    if let Some(existing_index) = seed.sessions.iter().position(|session| {
        session.circle_id == input.circle_id
            && session.contact_id.as_deref() == Some(&input.contact_id)
    }) {
        let mut session = seed.sessions.remove(existing_index);
        session.archived = Some(false);
        let session_id = session.id.clone();
        seed.sessions.insert(0, session);

        let seed = save_domain_seed(app_handle, seed)?;
        return Ok(StartConversationResult { seed, session_id });
    }

    let session_id =
        build_unique_session_id(&format!("session-{}", input.contact_id), &seed.sessions);
    seed.sessions.insert(
        0,
        SessionItem {
            id: session_id.clone(),
            circle_id: input.circle_id,
            name: contact.name.clone(),
            initials: contact.initials.clone(),
            subtitle: "Start a conversation".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::Direct,
            category: "friends".into(),
            members: None,
            contact_id: Some(contact.id.clone()),
            archived: None,
        },
    );
    seed.message_store.insert(
        session_id.clone(),
        vec![MessageItem {
            id: unique_local_id("system"),
            kind: MessageKind::System,
            author: MessageAuthor::System,
            body: format!("New conversation with {}", contact.name),
            time: String::new(),
            meta: None,
        }],
    );

    let seed = save_domain_seed(app_handle, seed)?;
    Ok(StartConversationResult { seed, session_id })
}

pub fn apply_session_action(
    app_handle: &tauri::AppHandle,
    input: SessionActionInput,
) -> Result<ChatDomainSeed, String> {
    let mut seed = load_domain_seed(app_handle)?;
    let target_index = seed
        .sessions
        .iter()
        .position(|session| session.id == input.session_id)
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;

    match input.action {
        ChatSessionAction::Pin => {
            let next_value = !seed.sessions[target_index].pinned.unwrap_or(false);
            seed.sessions[target_index].pinned = Some(next_value);
        }
        ChatSessionAction::Mute => {
            let next_value = !seed.sessions[target_index].muted.unwrap_or(false);
            seed.sessions[target_index].muted = Some(next_value);

            if matches!(seed.sessions[target_index].kind, SessionKind::Group) {
                for group in &mut seed.groups {
                    if group.session_id == input.session_id {
                        group.muted = Some(next_value);
                    }
                }
            }
        }
        ChatSessionAction::Archive => {
            seed.sessions[target_index].archived = Some(true);
            seed.sessions[target_index].pinned = Some(false);
        }
        ChatSessionAction::Unarchive => {
            let mut session = seed.sessions.remove(target_index);
            session.archived = Some(false);
            let session_id = session.id.clone();
            seed.sessions.insert(0, session);

            if let Some(messages) = seed.message_store.remove(&session_id) {
                seed.message_store.insert(session_id, messages);
            }
        }
        ChatSessionAction::Delete => {
            let session_id = seed.sessions[target_index].id.clone();
            seed.sessions.remove(target_index);
            seed.groups.retain(|group| group.session_id != session_id);
            seed.message_store.remove(&session_id);
        }
    }

    save_domain_seed(app_handle, seed)
}

pub fn toggle_contact_block(
    app_handle: &tauri::AppHandle,
    contact_id: String,
) -> Result<ChatDomainSeed, String> {
    let mut seed = load_domain_seed(app_handle)?;
    let contact = seed
        .contacts
        .iter_mut()
        .find(|item| item.id == contact_id)
        .ok_or_else(|| format!("contact not found: {contact_id}"))?;
    let next_value = !contact.blocked.unwrap_or(false);
    contact.blocked = Some(next_value);

    save_domain_seed(app_handle, seed)
}

pub fn add_circle(
    app_handle: &tauri::AppHandle,
    input: AddCircleInput,
) -> Result<AddCircleResult, String> {
    let mut seed = load_domain_seed(app_handle)?;
    let normalized_name = normalized_circle_name(&input);
    let normalized_relay = normalized_circle_relay(&input, &normalized_name);
    let existing_circle_id = seed
        .circles
        .iter()
        .find(|circle| {
            circle
                .relay
                .trim()
                .eq_ignore_ascii_case(normalized_relay.trim())
        })
        .map(|circle| circle.id.clone());

    if let Some(circle_id) = existing_circle_id {
        return Ok(AddCircleResult { seed, circle_id });
    }

    let circle_id = build_unique_circle_id(&normalized_name, &seed.circles);
    seed.circles.insert(
        0,
        CircleItem {
            id: circle_id.clone(),
            name: normalized_name,
            relay: normalized_relay,
            circle_type: match input.mode {
                CircleCreateMode::Private => CircleType::Paid,
                CircleCreateMode::Custom => CircleType::Custom,
                CircleCreateMode::Invite => CircleType::Default,
            },
            status: CircleStatus::Connecting,
            latency: "--".into(),
            description: match input.mode {
                CircleCreateMode::Private => {
                    "Private relay shell created from the onboarding flow.".into()
                }
                CircleCreateMode::Custom => {
                    "Custom relay connected from a manually entered endpoint.".into()
                }
                CircleCreateMode::Invite => {
                    "Circle imported from an invite handoff and waiting for relay confirmation."
                        .into()
                }
            },
        },
    );

    let seed = save_domain_seed(app_handle, seed)?;
    Ok(AddCircleResult { seed, circle_id })
}

pub fn update_circle(
    app_handle: &tauri::AppHandle,
    input: UpdateCircleInput,
) -> Result<ChatDomainSeed, String> {
    let mut seed = load_domain_seed(app_handle)?;
    let circle = seed
        .circles
        .iter_mut()
        .find(|item| item.id == input.circle_id)
        .ok_or_else(|| format!("circle not found: {}", input.circle_id))?;

    let next_name = if input.name.trim().is_empty() {
        circle.name.clone()
    } else {
        input.name.trim().to_string()
    };
    let next_description = if input.description.trim().is_empty() {
        circle.description.clone()
    } else {
        input.description.trim().to_string()
    };

    circle.name = next_name;
    circle.description = next_description;

    save_domain_seed(app_handle, seed)
}

pub fn remove_circle(
    app_handle: &tauri::AppHandle,
    circle_id: String,
) -> Result<ChatDomainSeed, String> {
    let mut seed = load_domain_seed(app_handle)?;
    if seed.circles.len() <= 1 {
        return Err("cannot remove the last circle".into());
    }

    if !seed.circles.iter().any(|circle| circle.id == circle_id) {
        return Err(format!("circle not found: {circle_id}"));
    }

    let removed_session_ids: HashSet<String> = seed
        .sessions
        .iter()
        .filter(|session| session.circle_id == circle_id)
        .map(|session| session.id.clone())
        .collect();

    seed.circles.retain(|circle| circle.id != circle_id);
    seed.sessions
        .retain(|session| session.circle_id != circle_id);
    seed.groups
        .retain(|group| !removed_session_ids.contains(&group.session_id));
    seed.message_store
        .retain(|session_id, _| !removed_session_ids.contains(session_id));

    save_domain_seed(app_handle, seed)
}

fn load_domain_seed(app_handle: &tauri::AppHandle) -> Result<ChatDomainSeed, String> {
    let repository = SqliteChatRepository::new(app_handle);
    Ok(repository.load_chat_seed()?.into())
}

fn save_domain_seed(
    app_handle: &tauri::AppHandle,
    seed: ChatDomainSeed,
) -> Result<ChatDomainSeed, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.save_chat_domain_seed(seed.clone())?;
    Ok(seed)
}

fn normalized_circle_name(input: &AddCircleInput) -> String {
    let trimmed = input.name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    match input.mode {
        CircleCreateMode::Private => "Private Circle".into(),
        CircleCreateMode::Custom => "Custom Relay".into(),
        CircleCreateMode::Invite => "Invite Circle".into(),
    }
}

fn normalized_circle_relay(input: &AddCircleInput, normalized_name: &str) -> String {
    match input.mode {
        CircleCreateMode::Private => format!(
            "wss://{}.private.circle.local",
            build_circle_slug(normalized_name)
        ),
        CircleCreateMode::Custom => input.relay.clone().unwrap_or_default().trim().to_string(),
        CircleCreateMode::Invite => {
            let invite_code = input.invite_code.clone().unwrap_or_default();
            if invite_code.trim().contains("://") {
                invite_code.trim().to_string()
            } else {
                format!("invite://{}", invite_code.trim())
            }
        }
    }
}

fn build_circle_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_separator = false;
            continue;
        }

        if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "circle".into()
    } else {
        slug
    }
}

fn build_unique_circle_id(base_label: &str, circles: &[CircleItem]) -> String {
    let base_id = build_circle_slug(base_label);
    let mut candidate = base_id.clone();
    let mut suffix = 2;

    while circles.iter().any(|circle| circle.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn build_unique_session_id(base_id: &str, sessions: &[SessionItem]) -> String {
    let mut candidate = base_id.to_string();
    let mut suffix = 2;

    while sessions.iter().any(|session| session.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn unique_local_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{prefix}-{millis}")
}
