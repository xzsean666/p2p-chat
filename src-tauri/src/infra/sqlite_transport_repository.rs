use crate::domain::transport::{
    DiscoveredPeer, PeerPresence, SessionSyncItem, SessionSyncState, TransportActivityItem,
    TransportActivityKind, TransportActivityLevel, TransportRuntimeAdapterKind,
    TransportRuntimeDesiredState, TransportRuntimeLaunchResult, TransportRuntimeLaunchStatus,
    TransportRuntimeQueueState, TransportRuntimeRecoveryPolicy, TransportRuntimeRegistryEntry,
    TransportRuntimeSession, TransportRuntimeState,
};
use crate::domain::transport_repository::{TransportCache, TransportRepository};
use crate::domain::transport_runtime_registry::{
    infer_runtime_registry_entry_from_legacy_session, project_runtime_session,
};
use crate::infra::sqlite_connection::open_connection;
use rusqlite::params;
use tauri::Runtime;

pub struct SqliteTransportRepository<R: Runtime> {
    app_handle: tauri::AppHandle<R>,
}

impl<R: Runtime> SqliteTransportRepository<R> {
    pub fn new(app_handle: &tauri::AppHandle<R>) -> Self {
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

            CREATE TABLE IF NOT EXISTS transport_runtime_session (
              circle_id TEXT PRIMARY KEY,
              driver TEXT NOT NULL,
              adapter_kind TEXT NOT NULL DEFAULT 'embedded',
              launch_status TEXT NOT NULL DEFAULT 'embedded',
              launch_command TEXT,
              launch_arguments_json TEXT NOT NULL DEFAULT '[]',
              resolved_launch_command TEXT,
              launch_error TEXT,
              last_launch_result TEXT,
              last_launch_pid INTEGER,
              last_launch_at TEXT,
              desired_state TEXT NOT NULL DEFAULT 'stopped',
              recovery_policy TEXT NOT NULL DEFAULT 'manual',
              queue_state TEXT NOT NULL DEFAULT 'idle',
              restart_attempts INTEGER NOT NULL DEFAULT 0,
              next_retry_in TEXT,
              next_retry_at_ms INTEGER,
              last_failure_reason TEXT,
              last_failure_at TEXT,
              state TEXT NOT NULL,
              generation INTEGER NOT NULL DEFAULT 0,
              state_since TEXT NOT NULL DEFAULT 'not started',
              session_label TEXT NOT NULL,
              endpoint TEXT NOT NULL,
              last_event TEXT NOT NULL,
              last_event_at TEXT NOT NULL DEFAULT 'not started'
            );

            CREATE TABLE IF NOT EXISTS transport_runtime_registry (
              circle_id TEXT PRIMARY KEY,
              driver TEXT NOT NULL,
              adapter_kind TEXT NOT NULL DEFAULT 'embedded',
              launch_status TEXT NOT NULL DEFAULT 'embedded',
              launch_command TEXT,
              launch_arguments_json TEXT NOT NULL DEFAULT '[]',
              resolved_launch_command TEXT,
              launch_error TEXT,
              last_launch_result TEXT,
              last_launch_pid INTEGER,
              last_launch_at TEXT,
              desired_state TEXT NOT NULL,
              recovery_policy TEXT NOT NULL,
              queue_state TEXT NOT NULL DEFAULT 'idle',
              restart_attempts INTEGER NOT NULL DEFAULT 0,
              next_retry_in TEXT,
              next_retry_at_ms INTEGER,
              last_failure_reason TEXT,
              last_failure_at TEXT,
              state TEXT NOT NULL,
              generation INTEGER NOT NULL DEFAULT 0,
              state_since TEXT NOT NULL DEFAULT 'not started',
              session_label TEXT NOT NULL,
              endpoint TEXT NOT NULL,
              last_event TEXT NOT NULL,
              last_event_at TEXT NOT NULL DEFAULT 'not started'
            );
            "#,
        )
        .map_err(|error| error.to_string())?;

        ensure_table_column(
            conn,
            "transport_runtime_session",
            "adapter_kind",
            "TEXT NOT NULL DEFAULT 'embedded'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "launch_status",
            "TEXT NOT NULL DEFAULT 'embedded'",
        )?;
        ensure_table_column(conn, "transport_runtime_session", "launch_command", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "launch_arguments_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "resolved_launch_command",
            "TEXT",
        )?;
        ensure_table_column(conn, "transport_runtime_session", "launch_error", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "last_launch_result",
            "TEXT",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "last_launch_pid",
            "INTEGER",
        )?;
        ensure_table_column(conn, "transport_runtime_session", "last_launch_at", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "desired_state",
            "TEXT NOT NULL DEFAULT 'stopped'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "recovery_policy",
            "TEXT NOT NULL DEFAULT 'manual'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "queue_state",
            "TEXT NOT NULL DEFAULT 'idle'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "restart_attempts",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_table_column(conn, "transport_runtime_session", "next_retry_in", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "next_retry_at_ms",
            "INTEGER",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "last_failure_reason",
            "TEXT",
        )?;
        ensure_table_column(conn, "transport_runtime_session", "last_failure_at", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "generation",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "state_since",
            "TEXT NOT NULL DEFAULT 'not started'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_session",
            "last_event_at",
            "TEXT NOT NULL DEFAULT 'not started'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "adapter_kind",
            "TEXT NOT NULL DEFAULT 'embedded'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "launch_status",
            "TEXT NOT NULL DEFAULT 'embedded'",
        )?;
        ensure_table_column(conn, "transport_runtime_registry", "launch_command", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "launch_arguments_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "resolved_launch_command",
            "TEXT",
        )?;
        ensure_table_column(conn, "transport_runtime_registry", "launch_error", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "last_launch_result",
            "TEXT",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "last_launch_pid",
            "INTEGER",
        )?;
        ensure_table_column(conn, "transport_runtime_registry", "last_launch_at", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "queue_state",
            "TEXT NOT NULL DEFAULT 'idle'",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "restart_attempts",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_table_column(conn, "transport_runtime_registry", "next_retry_in", "TEXT")?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "next_retry_at_ms",
            "INTEGER",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "last_failure_reason",
            "TEXT",
        )?;
        ensure_table_column(
            conn,
            "transport_runtime_registry",
            "last_failure_at",
            "TEXT",
        )?;

        Ok(())
    }
}

impl<R: Runtime> TransportRepository for SqliteTransportRepository<R> {
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

            let mut registry_stmt = conn
                .prepare(
                    "SELECT circle_id, driver, adapter_kind, launch_status, launch_command, launch_arguments_json, resolved_launch_command, launch_error, last_launch_result, last_launch_pid, last_launch_at, desired_state, recovery_policy, queue_state, restart_attempts, next_retry_in, next_retry_at_ms, last_failure_reason, last_failure_at, state, generation, state_since, session_label, endpoint, last_event, last_event_at FROM transport_runtime_registry ORDER BY rowid ASC",
                )
                .map_err(|error| error.to_string())?;
            let registry_rows = registry_stmt
                .query_map([], |row| {
                    Ok(TransportRuntimeRegistryEntry {
                        circle_id: row.get(0)?,
                        driver: row.get(1)?,
                        adapter_kind: transport_runtime_adapter_kind_from_str(
                            &row.get::<_, String>(2)?,
                        )
                        .map_err(sqlite_user_error)?,
                        launch_status: transport_runtime_launch_status_from_str(
                            &row.get::<_, String>(3)?,
                        )
                        .map_err(sqlite_user_error)?,
                        launch_command: row.get(4)?,
                        launch_arguments: transport_runtime_launch_arguments_from_json(
                            &row.get::<_, String>(5)?,
                        )
                        .map_err(sqlite_user_error)?,
                        resolved_launch_command: row.get(6)?,
                        launch_error: row.get(7)?,
                        last_launch_result: row
                            .get::<_, Option<String>>(8)?
                            .map(|value| transport_runtime_launch_result_from_str(&value))
                            .transpose()
                            .map_err(sqlite_user_error)?,
                        last_launch_pid: row
                            .get::<_, Option<i64>>(9)?
                            .map(i64_to_u32)
                            .transpose()
                            .map_err(sqlite_user_error)?,
                        last_launch_at: row.get(10)?,
                        desired_state: transport_runtime_desired_state_from_str(
                            &row.get::<_, String>(11)?,
                        )
                        .map_err(sqlite_user_error)?,
                        recovery_policy: transport_runtime_recovery_policy_from_str(
                            &row.get::<_, String>(12)?,
                        )
                        .map_err(sqlite_user_error)?,
                        queue_state: transport_runtime_queue_state_from_str(
                            &row.get::<_, String>(13)?,
                        )
                        .map_err(sqlite_user_error)?,
                        restart_attempts: row
                            .get::<_, i64>(14)
                            .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                        next_retry_in: row.get(15)?,
                        next_retry_at_ms: row
                            .get::<_, Option<i64>>(16)?
                            .map(i64_to_u64)
                            .transpose()
                            .map_err(sqlite_user_error)?,
                        last_failure_reason: row.get(17)?,
                        last_failure_at: row.get(18)?,
                        state: transport_runtime_state_from_str(&row.get::<_, String>(19)?)
                            .map_err(sqlite_user_error)?,
                        generation: row
                            .get::<_, i64>(20)
                            .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                        state_since: row.get(21)?,
                        session_label: row.get(22)?,
                        endpoint: row.get(23)?,
                        last_event: row.get(24)?,
                        last_event_at: row.get(25)?,
                    })
                })
                .map_err(|error| error.to_string())?;
            let mut runtime_registry = registry_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;

            if runtime_registry.is_empty() {
                let mut runtime_stmt = conn
                    .prepare(
                        "SELECT circle_id, driver, adapter_kind, launch_status, launch_command, launch_arguments_json, resolved_launch_command, launch_error, last_launch_result, last_launch_pid, last_launch_at, desired_state, recovery_policy, queue_state, restart_attempts, next_retry_in, next_retry_at_ms, last_failure_reason, last_failure_at, state, generation, state_since, session_label, endpoint, last_event, last_event_at FROM transport_runtime_session ORDER BY rowid ASC",
                    )
                    .map_err(|error| error.to_string())?;
                let runtime_rows = runtime_stmt
                    .query_map([], |row| {
                        Ok(TransportRuntimeSession {
                            circle_id: row.get(0)?,
                            driver: row.get(1)?,
                            adapter_kind: transport_runtime_adapter_kind_from_str(
                                &row.get::<_, String>(2)?,
                            )
                            .map_err(sqlite_user_error)?,
                            launch_status: transport_runtime_launch_status_from_str(
                                &row.get::<_, String>(3)?,
                            )
                            .map_err(sqlite_user_error)?,
                            launch_command: row.get(4)?,
                            launch_arguments: transport_runtime_launch_arguments_from_json(
                                &row.get::<_, String>(5)?,
                            )
                            .map_err(sqlite_user_error)?,
                            resolved_launch_command: row.get(6)?,
                            launch_error: row.get(7)?,
                            last_launch_result: row
                                .get::<_, Option<String>>(8)?
                                .map(|value| transport_runtime_launch_result_from_str(&value))
                                .transpose()
                                .map_err(sqlite_user_error)?,
                            last_launch_pid: row
                                .get::<_, Option<i64>>(9)?
                                .map(i64_to_u32)
                                .transpose()
                                .map_err(sqlite_user_error)?,
                            last_launch_at: row.get(10)?,
                            desired_state: transport_runtime_desired_state_from_str(
                                &row.get::<_, String>(11)?,
                            )
                            .map_err(sqlite_user_error)?,
                            recovery_policy: transport_runtime_recovery_policy_from_str(
                                &row.get::<_, String>(12)?,
                            )
                            .map_err(sqlite_user_error)?,
                            queue_state: transport_runtime_queue_state_from_str(
                                &row.get::<_, String>(13)?,
                            )
                            .map_err(sqlite_user_error)?,
                            restart_attempts: row
                                .get::<_, i64>(14)
                                .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                            next_retry_in: row.get(15)?,
                            next_retry_at_ms: row
                                .get::<_, Option<i64>>(16)?
                                .map(i64_to_u64)
                                .transpose()
                                .map_err(sqlite_user_error)?,
                            last_failure_reason: row.get(17)?,
                            last_failure_at: row.get(18)?,
                            state: transport_runtime_state_from_str(&row.get::<_, String>(19)?)
                                .map_err(sqlite_user_error)?,
                            generation: row
                                .get::<_, i64>(20)
                                .and_then(|value| i64_to_u32(value).map_err(sqlite_user_error))?,
                            state_since: row.get(21)?,
                            session_label: row.get(22)?,
                            endpoint: row.get(23)?,
                            last_event: row.get(24)?,
                            last_event_at: row.get(25)?,
                        })
                    })
                    .map_err(|error| error.to_string())?;
                runtime_registry = runtime_rows
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(infer_runtime_registry_entry_from_legacy_session)
                    .collect();
            }
            let runtime_sessions = runtime_registry
                .iter()
                .map(project_runtime_session)
                .collect::<Vec<_>>();

            Ok(TransportCache {
                peers,
                session_sync,
                activities,
                runtime_registry,
                runtime_sessions,
            })
        })
    }

    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String> {
        self.with_connection_mut(|conn| {
            let tx = conn.transaction().map_err(|error| error.to_string())?;
            tx.execute_batch(
                r#"
                DELETE FROM transport_activity;
                DELETE FROM transport_runtime_registry;
                DELETE FROM transport_runtime_session;
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

            for item in cache.runtime_registry {
                tx.execute(
                    "INSERT INTO transport_runtime_registry (circle_id, driver, adapter_kind, launch_status, launch_command, launch_arguments_json, resolved_launch_command, launch_error, last_launch_result, last_launch_pid, last_launch_at, desired_state, recovery_policy, queue_state, restart_attempts, next_retry_in, next_retry_at_ms, last_failure_reason, last_failure_at, state, generation, state_since, session_label, endpoint, last_event, last_event_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
                    params![
                        item.circle_id,
                        item.driver,
                        transport_runtime_adapter_kind_to_str(&item.adapter_kind),
                        transport_runtime_launch_status_to_str(&item.launch_status),
                        item.launch_command,
                        transport_runtime_launch_arguments_to_json(&item.launch_arguments)?,
                        item.resolved_launch_command,
                        item.launch_error,
                        item.last_launch_result
                            .as_ref()
                            .map(transport_runtime_launch_result_to_str),
                        item.last_launch_pid.map(i64::from),
                        item.last_launch_at,
                        transport_runtime_desired_state_to_str(&item.desired_state),
                        transport_runtime_recovery_policy_to_str(&item.recovery_policy),
                        transport_runtime_queue_state_to_str(&item.queue_state),
                        i64::from(item.restart_attempts),
                        item.next_retry_in,
                        item.next_retry_at_ms.map(|value| value as i64),
                        item.last_failure_reason,
                        item.last_failure_at,
                        transport_runtime_state_to_str(&item.state),
                        i64::from(item.generation),
                        item.state_since,
                        item.session_label,
                        item.endpoint,
                        item.last_event,
                        item.last_event_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
            }

            for item in cache.runtime_sessions {
                tx.execute(
                    "INSERT INTO transport_runtime_session (circle_id, driver, adapter_kind, launch_status, launch_command, launch_arguments_json, resolved_launch_command, launch_error, last_launch_result, last_launch_pid, last_launch_at, desired_state, recovery_policy, queue_state, restart_attempts, next_retry_in, next_retry_at_ms, last_failure_reason, last_failure_at, state, generation, state_since, session_label, endpoint, last_event, last_event_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
                    params![
                        item.circle_id,
                        item.driver,
                        transport_runtime_adapter_kind_to_str(&item.adapter_kind),
                        transport_runtime_launch_status_to_str(&item.launch_status),
                        item.launch_command,
                        transport_runtime_launch_arguments_to_json(&item.launch_arguments)?,
                        item.resolved_launch_command,
                        item.launch_error,
                        item.last_launch_result
                            .as_ref()
                            .map(transport_runtime_launch_result_to_str),
                        item.last_launch_pid.map(i64::from),
                        item.last_launch_at,
                        transport_runtime_desired_state_to_str(&item.desired_state),
                        transport_runtime_recovery_policy_to_str(&item.recovery_policy),
                        transport_runtime_queue_state_to_str(&item.queue_state),
                        i64::from(item.restart_attempts),
                        item.next_retry_in,
                        item.next_retry_at_ms.map(|value| value as i64),
                        item.last_failure_reason,
                        item.last_failure_at,
                        transport_runtime_state_to_str(&item.state),
                        i64::from(item.generation),
                        item.state_since,
                        item.session_label,
                        item.endpoint,
                        item.last_event,
                        item.last_event_at,
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

fn i64_to_u64(value: i64) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("invalid integer value: {value}"))
}

fn ensure_table_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    let has_column = columns
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|name| name == column);

    if has_column {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
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

fn transport_runtime_state_to_str(value: &TransportRuntimeState) -> &'static str {
    match value {
        TransportRuntimeState::Inactive => "inactive",
        TransportRuntimeState::Starting => "starting",
        TransportRuntimeState::Active => "active",
    }
}

fn transport_runtime_state_from_str(value: &str) -> Result<TransportRuntimeState, String> {
    match value {
        "inactive" => Ok(TransportRuntimeState::Inactive),
        "starting" => Ok(TransportRuntimeState::Starting),
        "active" => Ok(TransportRuntimeState::Active),
        _ => Err(format!("unknown transport runtime state: {value}")),
    }
}

fn transport_runtime_desired_state_to_str(value: &TransportRuntimeDesiredState) -> &'static str {
    match value {
        TransportRuntimeDesiredState::Stopped => "stopped",
        TransportRuntimeDesiredState::Running => "running",
    }
}

fn transport_runtime_desired_state_from_str(
    value: &str,
) -> Result<TransportRuntimeDesiredState, String> {
    match value {
        "stopped" => Ok(TransportRuntimeDesiredState::Stopped),
        "running" => Ok(TransportRuntimeDesiredState::Running),
        _ => Err(format!("unknown transport runtime desired state: {value}")),
    }
}

fn transport_runtime_recovery_policy_to_str(
    value: &TransportRuntimeRecoveryPolicy,
) -> &'static str {
    match value {
        TransportRuntimeRecoveryPolicy::Manual => "manual",
        TransportRuntimeRecoveryPolicy::Auto => "auto",
    }
}

fn transport_runtime_recovery_policy_from_str(
    value: &str,
) -> Result<TransportRuntimeRecoveryPolicy, String> {
    match value {
        "manual" => Ok(TransportRuntimeRecoveryPolicy::Manual),
        "auto" => Ok(TransportRuntimeRecoveryPolicy::Auto),
        _ => Err(format!(
            "unknown transport runtime recovery policy: {value}"
        )),
    }
}

fn transport_runtime_queue_state_to_str(value: &TransportRuntimeQueueState) -> &'static str {
    match value {
        TransportRuntimeQueueState::Idle => "idle",
        TransportRuntimeQueueState::Queued => "queued",
        TransportRuntimeQueueState::Backoff => "backoff",
    }
}

fn transport_runtime_queue_state_from_str(
    value: &str,
) -> Result<TransportRuntimeQueueState, String> {
    match value {
        "idle" => Ok(TransportRuntimeQueueState::Idle),
        "queued" => Ok(TransportRuntimeQueueState::Queued),
        "backoff" => Ok(TransportRuntimeQueueState::Backoff),
        _ => Err(format!("unknown transport runtime queue state: {value}")),
    }
}

fn transport_runtime_adapter_kind_to_str(value: &TransportRuntimeAdapterKind) -> &'static str {
    match value {
        TransportRuntimeAdapterKind::Embedded => "embedded",
        TransportRuntimeAdapterKind::LocalCommand => "localCommand",
    }
}

fn transport_runtime_adapter_kind_from_str(
    value: &str,
) -> Result<TransportRuntimeAdapterKind, String> {
    match value {
        "embedded" => Ok(TransportRuntimeAdapterKind::Embedded),
        "localCommand" => Ok(TransportRuntimeAdapterKind::LocalCommand),
        _ => Err(format!("unknown transport runtime adapter kind: {value}")),
    }
}

fn transport_runtime_launch_status_to_str(value: &TransportRuntimeLaunchStatus) -> &'static str {
    match value {
        TransportRuntimeLaunchStatus::Embedded => "embedded",
        TransportRuntimeLaunchStatus::Ready => "ready",
        TransportRuntimeLaunchStatus::Missing => "missing",
        TransportRuntimeLaunchStatus::Unknown => "unknown",
    }
}

fn transport_runtime_launch_status_from_str(
    value: &str,
) -> Result<TransportRuntimeLaunchStatus, String> {
    match value {
        "embedded" => Ok(TransportRuntimeLaunchStatus::Embedded),
        "ready" => Ok(TransportRuntimeLaunchStatus::Ready),
        "missing" => Ok(TransportRuntimeLaunchStatus::Missing),
        "unknown" => Ok(TransportRuntimeLaunchStatus::Unknown),
        _ => Err(format!("unknown transport runtime launch status: {value}")),
    }
}

fn transport_runtime_launch_result_to_str(value: &TransportRuntimeLaunchResult) -> &'static str {
    match value {
        TransportRuntimeLaunchResult::Spawned => "spawned",
        TransportRuntimeLaunchResult::Reused => "reused",
        TransportRuntimeLaunchResult::Failed => "failed",
    }
}

fn transport_runtime_launch_result_from_str(
    value: &str,
) -> Result<TransportRuntimeLaunchResult, String> {
    match value {
        "spawned" => Ok(TransportRuntimeLaunchResult::Spawned),
        "reused" => Ok(TransportRuntimeLaunchResult::Reused),
        "failed" => Ok(TransportRuntimeLaunchResult::Failed),
        _ => Err(format!("unknown transport runtime launch result: {value}")),
    }
}

fn transport_runtime_launch_arguments_to_json(value: &[String]) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn transport_runtime_launch_arguments_from_json(value: &str) -> Result<Vec<String>, String> {
    serde_json::from_str(value).map_err(|error| error.to_string())
}
