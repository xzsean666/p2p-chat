use crate::domain::chat::{
    AdvancedPreferences, AppPreferences, ChatDomainSeed, CircleItem, CircleStatus, CircleType,
    ContactItem, GroupMember, GroupProfile, GroupRole, LanguagePreference, MessageAuthor,
    MessageItem, MessageKind, NotificationPreferences, PersistedShellState, SessionItem,
    SessionKind, TextSizePreference, ThemePreference,
};
use crate::domain::chat_repository::ChatRepository;
use std::collections::HashMap;

#[derive(Default)]
pub struct SeedChatRepository;

impl ChatRepository for SeedChatRepository {
    fn load_chat_seed(&self) -> Result<PersistedShellState, String> {
        Ok(PersistedShellState {
            is_authenticated: false,
            circles: self.load_seed_circles()?,
            app_preferences: AppPreferences {
                theme: ThemePreference::System,
                language: LanguagePreference::En,
                text_size: TextSizePreference::Default,
            },
            notification_preferences: NotificationPreferences {
                allow_send: true,
                allow_receive: false,
                show_badge: true,
                archive_summary: true,
                mentions_only: false,
            },
            advanced_preferences: AdvancedPreferences {
                show_message_info: false,
                use_tor_network: false,
                relay_diagnostics: true,
                experimental_transport: false,
            },
            active_circle_id: "main-circle".into(),
            selected_session_id: String::new(),
            sessions: self.load_seed_sessions()?,
            contacts: self.load_seed_contacts()?,
            groups: self.load_seed_groups()?,
            message_store: self.load_seed_message_store()?,
        })
    }

    fn load_seed_circles(&self) -> Result<Vec<CircleItem>, String> {
        Ok(vec![
            CircleItem {
                id: "main-circle".into(),
                name: "Main Circle".into(),
                relay: "relay.p2p-chat.local".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Open,
                latency: "48 ms".into(),
                description: "Primary relay and session hub".into(),
            },
            CircleItem {
                id: "paid-circle".into(),
                name: "Studio Circle".into(),
                relay: "studio.circle.local".into(),
                circle_type: CircleType::Paid,
                status: CircleStatus::Connecting,
                latency: "--".into(),
                description: "Private paid relay for the product team".into(),
            },
            CircleItem {
                id: "bitchat-circle".into(),
                name: "BitChat".into(),
                relay: "mesh://local".into(),
                circle_type: CircleType::Bitchat,
                status: CircleStatus::Closed,
                latency: "--".into(),
                description: "Offline mesh-style relay space".into(),
            },
        ])
    }

    fn load_seed_contacts(&self) -> Result<Vec<ContactItem>, String> {
        Ok(vec![
            contact(
                "alice-contact",
                "Alice Chen",
                "AC",
                "@alice",
                "npub1alice7f3x0f7cehmfxf6j2pchat",
                "Product design",
                "Designs the messaging flows and keeps the interaction model tight.",
                Some(true),
            ),
            contact(
                "mika-contact",
                "Mika Torres",
                "MT",
                "@mika",
                "npub1mika8m2x0c4f7uew3d0p2pchat",
                "Frontend engineering",
                "Owns the Vue shell and interaction quality on desktop.",
                Some(true),
            ),
            contact(
                "release-contact",
                "Release Bot",
                "RB",
                "@releasebot",
                "npub1releasebot39d1c8f0buildstatus",
                "Automation",
                "Publishes build status, packaging output and pipeline alerts.",
                None,
            ),
            contact(
                "relay-contact",
                "Relay Ops",
                "RO",
                "@relayops",
                "npub1relayops8d0k2a0m7s1infra",
                "Infrastructure",
                "Tracks relay health, latency and circle connectivity.",
                None,
            ),
            contact(
                "nora-contact",
                "Nora Blake",
                "NB",
                "@nora",
                "npub1nora9h2e0n2j9q4research",
                "Research",
                "Reviews flows and validates parity against the source app.",
                Some(true),
            ),
            contact(
                "oliver-contact",
                "Oliver Grant",
                "OG",
                "@oliver",
                "npub1oliver7r3d6k2archivecase",
                "QA",
                "Verifies archived sessions, mute flows and edge cases.",
                None,
            ),
        ])
    }

    fn load_seed_sessions(&self) -> Result<Vec<SessionItem>, String> {
        Ok(vec![
            SessionItem {
                id: "alice".into(),
                circle_id: "main-circle".into(),
                contact_id: Some("alice-contact".into()),
                name: "Alice Chen".into(),
                initials: "AC".into(),
                subtitle: "The desktop shell already reads a lot like XChat.".into(),
                time: "12:48".into(),
                unread_count: Some(2),
                pinned: Some(true),
                muted: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                archived: None,
            },
            SessionItem {
                id: "design".into(),
                circle_id: "main-circle".into(),
                contact_id: None,
                name: "Design Circle".into(),
                initials: "DC".into(),
                subtitle: "Updated group icon set and composer spacing.".into(),
                time: "11:15".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::Group,
                category: "groups".into(),
                members: Some(12),
                archived: None,
            },
            SessionItem {
                id: "assistant".into(),
                circle_id: "main-circle".into(),
                contact_id: None,
                name: "File Transfer Assistant".into(),
                initials: "FA".into(),
                subtitle: "Pinned notes and exported files stay here.".into(),
                time: "Yesterday".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::SelfChat,
                category: "system".into(),
                members: None,
                archived: None,
            },
            SessionItem {
                id: "release".into(),
                circle_id: "main-circle".into(),
                contact_id: Some("release-contact".into()),
                name: "Release Bot".into(),
                initials: "RB".into(),
                subtitle: "Desktop build validation finished.".into(),
                time: "Yesterday".into(),
                unread_count: None,
                pinned: None,
                muted: Some(true),
                draft: None,
                kind: SessionKind::Direct,
                category: "system".into(),
                members: None,
                archived: None,
            },
            SessionItem {
                id: "relay".into(),
                circle_id: "main-circle".into(),
                contact_id: Some("relay-contact".into()),
                name: "Relay Ops".into(),
                initials: "RO".into(),
                subtitle: "Latency stable across the current circle.".into(),
                time: "Mon".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "ops".into(),
                members: None,
                archived: Some(true),
            },
            SessionItem {
                id: "mika".into(),
                circle_id: "main-circle".into(),
                contact_id: Some("mika-contact".into()),
                name: "Mika Torres".into(),
                initials: "MT".into(),
                subtitle: "Can you match the call actions in the header too?".into(),
                time: "Mon".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: Some("[Draft] Finish PrimeVue shell polish".into()),
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                archived: None,
            },
            SessionItem {
                id: "studio".into(),
                circle_id: "paid-circle".into(),
                contact_id: None,
                name: "Studio Circle".into(),
                initials: "SC".into(),
                subtitle: "Private relay onboarding starts after signer verification.".into(),
                time: "09:18".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::Group,
                category: "groups".into(),
                members: Some(7),
                archived: None,
            },
            SessionItem {
                id: "nora".into(),
                circle_id: "paid-circle".into(),
                contact_id: Some("nora-contact".into()),
                name: "Nora Blake".into(),
                initials: "NB".into(),
                subtitle: "Use the circle dropdown to swap relay contexts.".into(),
                time: "08:54".into(),
                unread_count: Some(1),
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                archived: None,
            },
            SessionItem {
                id: "oliver".into(),
                circle_id: "paid-circle".into(),
                contact_id: Some("oliver-contact".into()),
                name: "Oliver Grant".into(),
                initials: "OG".into(),
                subtitle: "Archived regression flow matches the source app now.".into(),
                time: "Sun".into(),
                unread_count: None,
                pinned: None,
                muted: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "qa".into(),
                members: None,
                archived: Some(true),
            },
        ])
    }

    fn load_seed_groups(&self) -> Result<Vec<GroupProfile>, String> {
        Ok(vec![
            GroupProfile {
                session_id: "design".into(),
                name: "Design Circle".into(),
                description: "Core product and design discussion space for shell parity work."
                    .into(),
                members: vec![
                    member("alice-contact", Some(GroupRole::Admin)),
                    member("mika-contact", Some(GroupRole::Member)),
                    member("nora-contact", Some(GroupRole::Member)),
                ],
                muted: None,
            },
            GroupProfile {
                session_id: "studio".into(),
                name: "Studio Circle".into(),
                description:
                    "Private relay group used to validate paid-circle behavior and onboarding."
                        .into(),
                members: vec![
                    member("nora-contact", Some(GroupRole::Admin)),
                    member("alice-contact", Some(GroupRole::Member)),
                    member("mika-contact", Some(GroupRole::Member)),
                    member("oliver-contact", Some(GroupRole::Member)),
                ],
                muted: Some(true),
            },
        ])
    }

    fn load_seed_message_store(&self) -> Result<HashMap<String, Vec<MessageItem>>, String> {
        let mut message_store = HashMap::new();
        message_store.insert(
            "alice".into(),
            vec![
                system_message("alice-system", "Today · Apr 15, 2026"),
                text_message(
                    "alice-1",
                    MessageAuthor::Peer,
                    "The new desktop shell already reads a lot like the original app.",
                    "12:37",
                ),
                text_message(
                    "alice-2",
                    MessageAuthor::Me,
                    "I matched the session list density, the circular avatars, the header actions and the self-chat icon first.",
                    "12:39",
                ),
                file_message("alice-3", "xchat-desktop-shell-notes.pdf", "482 KB", "12:41"),
                audio_message("alice-4", "Voice note", "00:28", "12:43"),
                text_message(
                    "alice-5",
                    MessageAuthor::Peer,
                    "Good. Keep the unread badge behavior and circle picker close to the source app.",
                    "12:48",
                ),
            ],
        );
        message_store.insert(
            "design".into(),
            vec![
                system_message("design-system", "Design Circle · 12 members"),
                text_message(
                    "design-1",
                    MessageAuthor::Peer,
                    "The desktop version should keep the same clean spacing as the session list on mobile.",
                    "11:05",
                ),
            ],
        );
        message_store.insert(
            "assistant".into(),
            vec![
                system_message("assistant-system", "Private note space"),
                text_message(
                    "assistant-1",
                    MessageAuthor::Me,
                    "PrimeVue shell initialized. Next: route scaffolding, real store and Rust-backed data.",
                    "Yesterday",
                ),
            ],
        );
        message_store.insert(
            "release".into(),
            vec![text_message(
                "release-1",
                MessageAuthor::Peer,
                "Desktop shell build succeeded after the theme and component split.",
                "Yesterday",
            )],
        );
        message_store.insert(
            "relay".into(),
            vec![
                system_message("relay-system", "Archived conversation"),
                text_message(
                    "relay-1",
                    MessageAuthor::Peer,
                    "Move this to archived after the current sprint and keep restore one click away.",
                    "Mon",
                ),
            ],
        );
        message_store.insert(
            "mika".into(),
            vec![text_message(
                "mika-1",
                MessageAuthor::Peer,
                "Use PrimeVue, but keep the layout close to the original XChat shell.",
                "Mon",
            )],
        );
        message_store.insert(
            "studio".into(),
            vec![
                system_message("studio-system", "Studio Circle · private relay"),
                text_message(
                    "studio-1",
                    MessageAuthor::Peer,
                    "This circle should mimic the paid relay state and empty-state handling.",
                    "09:18",
                ),
            ],
        );
        message_store.insert(
            "nora".into(),
            vec![text_message(
                "nora-1",
                MessageAuthor::Peer,
                "The settings drawer should show circles first, then preferences and help.",
                "08:54",
            )],
        );
        message_store.insert(
            "oliver".into(),
            vec![
                system_message("oliver-system", "Archived conversation"),
                text_message(
                    "oliver-1",
                    MessageAuthor::Peer,
                    "Regression check passed for archived list, unarchive and reopen.",
                    "Sun",
                ),
            ],
        );

        Ok(message_store)
    }

    fn save_chat_domain_seed(&self, _seed: ChatDomainSeed) -> Result<(), String> {
        Err("seed repository is read-only".into())
    }
}

fn contact(
    id: &str,
    name: &str,
    initials: &str,
    handle: &str,
    pubkey: &str,
    subtitle: &str,
    bio: &str,
    online: Option<bool>,
) -> ContactItem {
    ContactItem {
        id: id.into(),
        name: name.into(),
        initials: initials.into(),
        handle: handle.into(),
        pubkey: pubkey.into(),
        subtitle: subtitle.into(),
        bio: bio.into(),
        online,
        blocked: None,
    }
}

fn member(contact_id: &str, role: Option<GroupRole>) -> GroupMember {
    GroupMember {
        contact_id: contact_id.into(),
        role,
    }
}

fn system_message(id: &str, body: &str) -> MessageItem {
    MessageItem {
        id: id.into(),
        kind: MessageKind::System,
        author: MessageAuthor::System,
        body: body.into(),
        time: String::new(),
        meta: None,
    }
}

fn text_message(id: &str, author: MessageAuthor, body: &str, time: &str) -> MessageItem {
    MessageItem {
        id: id.into(),
        kind: MessageKind::Text,
        author,
        body: body.into(),
        time: time.into(),
        meta: None,
    }
}

fn file_message(id: &str, body: &str, meta: &str, time: &str) -> MessageItem {
    MessageItem {
        id: id.into(),
        kind: MessageKind::File,
        author: MessageAuthor::Peer,
        body: body.into(),
        time: time.into(),
        meta: Some(meta.into()),
    }
}

fn audio_message(id: &str, body: &str, meta: &str, time: &str) -> MessageItem {
    MessageItem {
        id: id.into(),
        kind: MessageKind::Audio,
        author: MessageAuthor::Me,
        body: body.into(),
        time: time.into(),
        meta: Some(meta.into()),
    }
}
