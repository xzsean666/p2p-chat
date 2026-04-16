use crate::domain::transport::{
    DiscoveredPeer, PeerPresence, SessionSyncItem, SessionSyncState, TransportActivityItem,
    TransportActivityKind, TransportActivityLevel,
};
use crate::domain::transport_repository::{TransportCache, TransportRepository};
use crate::infra::sqlite_connection::open_connection;
use rusqlite::params;

pub struct SqliteTransportRepository {
    app_handle: tauri::AppHandle,
}

impl SqliteTransportRepository {
    pub fn new(app_handle: &tauri::AppHandle) -> Self {
        Self {
            app_handle: app_handle.clone(),
        }
    }

    fn with_connection<T>(
        &self,
        handler: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let conn = open_connection(&self.app_handle)?;
        self.ensure_schema(&conn)?;
        handler(&conn)
    }

    fn with_connection_mut<T>(
        &self,
        handler: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut conn = open_connection(&self.app_handle)?;
        self.ensure_schema(&conn)?;
        handler(&mut conn)
    }

    fn ensure_schema(&self, conn: &rusqlite::Connection) -> Result<(), String> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS transport_peers (
              circle_id TEXT NOT NULL,
              contact_id TEXT NOT NULL,
              name TEXT NOT NULL,
              handle TEXT NOT NULL,
              presence TEXT NOT NULL,
              route TEXT NOT NULL,
              shared_sessions INTEGER NOT NULL,
              last_seen TEXT NOT NULL,
              blocked INTEGER NOT NULL,
              PRIMARY KEY (circle_id, contact_id)
            );

            CREATE TABLE IF NOT EXISTS transport_session_sync (
              circle_id TEXT NOT NULL,
              session_id TEXT NOT NULL,
              session_name TEXT NOT NULL,
              state TEXT NOT NULL,
              pending_messages INTEGER NOT NULL,
              source TEXT NOT NULL,
              last_merge TEXT NOT NULL,
              PRIMARY KEY (circle_id, session_id)
            );

            CREATE TABLE IF NOT EXISTS transport_activity (
              id TEXT PRIMARY KEY,
              position INTEGER NOT NULL,
              circle_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              level TEXT NOT NULL,
              title TEXT NOT NULL,
              detail TEXT NOT NULL,
              time TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())
    }
}

impl TransportRepository for SqliteTransportRepository {
    fn load_transport_cache(&self) -> Result<TransportCache, String> {
        self.with_connection(|conn| {
            let mut peer_stmt = conn
                .prepare(
                    "SELECT circle_id, contact_id, name, handle, presence, route, shared_sessions, last_seen, blocked FROM transport_peers ORDER BY rowid ASC",
                )
                .map_err(|error| error.to_string())?;
            let peer_rows = peer_stmt
                .query_map([], |row| {
                    Ok(DiscoveredPeer {
                        circle_id: row.get(0)?,
                        contact_id: row.get(1)?,
                        name: row.get(2)?,
                        handle: row.get(3)?,
                        presence: peer_presence_from_str(&row.get::<_, String>(4)?)
                            .map_err(sqlite_user_error)?,
                        route: row.get(5)?,
                        shared_sessions: row
                            .get::<_, i64>(6)
                            .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                        last_seen: row.get(7)?,
                        blocked: row.get::<_, i64>(8)? != 0,
                    })
                })
                .map_err(|error| error.to_string())?;
            let peers = peer_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;

            let mut sync_stmt = conn
                .prepare(
                    "SELECT circle_id, session_id, session_name, state, pending_messages, source, last_merge FROM transport_session_sync ORDER BY rowid ASC",
                )
                .map_err(|error| error.to_string())?;
            let sync_rows = sync_stmt
                .query_map([], |row| {
                    Ok(SessionSyncItem {
                        circle_id: row.get(0)?,
                        session_id: row.get(1)?,
                        session_name: row.get(2)?,
                        state: session_sync_state_from_str(&row.get::<_, String>(3)?)
                            .map_err(sqlite_user_error)?,
                        pending_messages: row
                            .get::<_, i64>(4)
                            .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                        source: row.get(5)?,
                        last_merge: row.get(6)?,
                    })
                })
                .map_err(|error| error.to_string())?;
            let session_sync = sync_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;

            let mut activity_stmt = conn
                .prepare(
                    "SELECT id, circle_id, kind, level, title, detail, time FROM transport_activity ORDER BY position ASC, rowid ASC",
                )
                .map_err(|error| error.to_string())?;
            let activity_rows = activity_stmt
                .query_map([], |row| {
                    Ok(TransportActivityItem {
                        id: row.get(0)?,
                        circle_id: row.get(1)?,
                        kind: transport_activity_kind_from_str(&row.get::<_, String>(2)?)
                            .map_err(sqlite_user_error)?,
                        level: transport_activity_level_from_str(&row.get::<_, String>(3)?)
                            .map_err(sqlite_user_error)?,
                        title: row.get(4)?,
                        detail: row.get(5)?,
                        time: row.get(6)?,
                    })
                })
                .map_err(|error| error.to_string())?;
            let activities = activity_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;

            Ok(TransportCache {
                peers,
                session_sync,
                activities,
            })
        })
    }

    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String> {
        self.with_connection_mut(|conn| {
            let tx = conn.transaction().map_err(|error| error.to_string())?;
            tx.execute_batch(
                r#"
                DELETE FROM transport_activity;
                DELETE FROM transport_session_sync;
                DELETE FROM transport_peers;
                "#,
            )
            .map_err(|error| error.to_string())?;

            for peer in cache.peers {
                tx.execute(
                    "INSERT INTO transport_peers (circle_id, contact_id, name, handle, presence, route, shared_sessions, last_seen, blocked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        peer.circle_id,
                        peer.contact_id,
                        peer.name,
                        peer.handle,
                        peer_presence_to_str(&peer.presence),
                        peer.route,
                        i64::from(peer.shared_sessions),
                        peer.last_seen,
                        bool_to_i64(peer.blocked),
                    ],
                )
                .map_err(|error| error.to_string())?;
            }

            for item in cache.session_sync {
                tx.execute(
                    "INSERT INTO transport_session_sync (circle_id, session_id, session_name, state, pending_messages, source, last_merge) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        item.circle_id,
                        item.session_id,
                        item.session_name,
                        session_sync_state_to_str(&item.state),
                        i64::from(item.pending_messages),
                        item.source,
                        item.last_merge,
                    ],
                )
                .map_err(|error| error.to_string())?;
            }

            for (position, item) in cache.activities.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO transport_activity (id, position, circle_id, kind, level, title, detail, time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        item.id,
                        position as i64,
                        item.circle_id,
                        transport_activity_kind_to_str(&item.kind),
                        transport_activity_level_to_str(&item.level),
                        item.title,
                        item.detail,
                        item.time,
                    ],
                )
                .map_err(|error| error.to_string())?;
            }

            tx.commit().map_err(|error| error.to_string())
        })
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_u32(value: i64) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("invalid integer value: {value}"))
}

fn sqlite_user_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn peer_presence_to_str(value: &PeerPresence) -> &'static str {
    match value {
        PeerPresence::Online => "online",
        PeerPresence::Idle => "idle",
        PeerPresence::Offline => "offline",
    }
}

fn peer_presence_from_str(value: &str) -> Result<PeerPresence, String> {
    match value {
        "online" => Ok(PeerPresence::Online),
        "idle" => Ok(PeerPresence::Idle),
        "offline" => Ok(PeerPresence::Offline),
        _ => Err(format!("unknown peer presence: {value}")),
    }
}

fn session_sync_state_to_str(value: &SessionSyncState) -> &'static str {
    match value {
        SessionSyncState::Idle => "idle",
        SessionSyncState::Syncing => "syncing",
        SessionSyncState::Pending => "pending",
        SessionSyncState::Conflict => "conflict",
    }
}

fn session_sync_state_from_str(value: &str) -> Result<SessionSyncState, String> {
    match value {
        "idle" => Ok(SessionSyncState::Idle),
        "syncing" => Ok(SessionSyncState::Syncing),
        "pending" => Ok(SessionSyncState::Pending),
        "conflict" => Ok(SessionSyncState::Conflict),
        _ => Err(format!("unknown session sync state: {value}")),
    }
}

fn transport_activity_kind_to_str(value: &TransportActivityKind) -> &'static str {
    match value {
        TransportActivityKind::Runtime => "runtime",
        TransportActivityKind::Connect => "connect",
        TransportActivityKind::Disconnect => "disconnect",
        TransportActivityKind::Sync => "sync",
        TransportActivityKind::DiscoverPeers => "discoverPeers",
        TransportActivityKind::SyncSessions => "syncSessions",
    }
}

fn transport_activity_kind_from_str(value: &str) -> Result<TransportActivityKind, String> {
    match value {
        "runtime" => Ok(TransportActivityKind::Runtime),
        "connect" => Ok(TransportActivityKind::Connect),
        "disconnect" => Ok(TransportActivityKind::Disconnect),
        "sync" => Ok(TransportActivityKind::Sync),
        "discoverPeers" => Ok(TransportActivityKind::DiscoverPeers),
        "syncSessions" => Ok(TransportActivityKind::SyncSessions),
        _ => Err(format!("unknown transport activity kind: {value}")),
    }
}

fn transport_activity_level_to_str(value: &TransportActivityLevel) -> &'static str {
    match value {
        TransportActivityLevel::Info => "info",
        TransportActivityLevel::Success => "success",
        TransportActivityLevel::Warn => "warn",
    }
}

fn transport_activity_level_from_str(value: &str) -> Result<TransportActivityLevel, String> {
    match value {
        "info" => Ok(TransportActivityLevel::Info),
        "success" => Ok(TransportActivityLevel::Success),
        "warn" => Ok(TransportActivityLevel::Warn),
        _ => Err(format!("unknown transport activity level: {value}")),
    }
}
