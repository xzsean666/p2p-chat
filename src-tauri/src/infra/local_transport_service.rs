use crate::domain::chat::{ChatDomainSeed, CircleType};
use crate::domain::chat_repository::ChatRepository;
use crate::domain::transport::{
    CircleTransportDiagnostic, RelayProtocol, TransportCapabilities, TransportCircleActionInput,
    TransportEngineKind, TransportHealth, TransportMutationResult, TransportService,
    TransportSnapshot, TransportSnapshotInput,
};
use crate::domain::transport_engine::overall_transport_health;
use crate::domain::transport_repository::{TransportCache, TransportRepository};
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use crate::infra::sqlite_transport_repository::SqliteTransportRepository;
use crate::infra::transport_engine_factory::select_transport_engine;

pub struct LocalTransportService {
    app_handle: tauri::AppHandle,
}

impl LocalTransportService {
    pub fn new(app_handle: &tauri::AppHandle) -> Self {
        Self {
            app_handle: app_handle.clone(),
        }
    }
}

impl TransportService for LocalTransportService {
    fn load_snapshot(&self, input: TransportSnapshotInput) -> Result<TransportSnapshot, String> {
        let seed = self.load_seed()?;
        let previous_cache = self.load_transport_cache()?;
        let engine = select_transport_engine(input.experimental_transport);
        let state = engine.build_state(&seed, &previous_cache)?;
        let snapshot = build_transport_snapshot(
            &seed,
            input,
            state.kind.clone(),
            state.diagnostics,
            state.cache.clone(),
        );
        self.save_transport_cache(state.cache)?;
        Ok(snapshot)
    }

    fn apply_circle_action(
        &self,
        input: TransportCircleActionInput,
    ) -> Result<TransportMutationResult, String> {
        let mut seed = self.load_seed()?;
        let previous_cache = self.load_transport_cache()?;
        let engine = select_transport_engine(input.experimental_transport);
        let state = engine.apply_circle_action(&mut seed, &previous_cache, &input)?;
        self.save_seed(&seed)?;
        self.save_transport_cache(state.cache.clone())?;

        let snapshot = build_transport_snapshot(
            &seed,
            TransportSnapshotInput {
                active_circle_id: input.active_circle_id.or(Some(input.circle_id)),
                use_tor_network: input.use_tor_network,
                experimental_transport: input.experimental_transport,
            },
            state.kind,
            state.diagnostics,
            state.cache,
        );

        Ok(TransportMutationResult { seed, snapshot })
    }
}

impl LocalTransportService {
    fn load_seed(&self) -> Result<ChatDomainSeed, String> {
        let repository = SqliteChatRepository::new(&self.app_handle);
        Ok(repository.load_chat_seed()?.into())
    }

    fn save_seed(&self, seed: &ChatDomainSeed) -> Result<(), String> {
        let repository = SqliteChatRepository::new(&self.app_handle);
        repository.save_chat_domain_seed(seed.clone())
    }

    fn load_transport_cache(&self) -> Result<TransportCache, String> {
        let repository = SqliteTransportRepository::new(&self.app_handle);
        repository.load_transport_cache()
    }

    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String> {
        let repository = SqliteTransportRepository::new(&self.app_handle);
        repository.save_transport_cache(cache)
    }
}

fn build_transport_snapshot(
    seed: &ChatDomainSeed,
    input: TransportSnapshotInput,
    engine: TransportEngineKind,
    diagnostics: Vec<CircleTransportDiagnostic>,
    cache: TransportCache,
) -> TransportSnapshot {
    let connected_relays = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.health, TransportHealth::Online))
        .count() as u32;
    let queued_messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.queued_messages)
        .sum();
    let active_circle_id = input
        .active_circle_id
        .filter(|circle_id| diagnostics.iter().any(|item| item.circle_id == *circle_id))
        .or_else(|| diagnostics.first().map(|item| item.circle_id.clone()))
        .unwrap_or_default();

    TransportSnapshot {
        engine,
        status: overall_transport_health(&diagnostics),
        active_circle_id,
        relay_count: diagnostics.len() as u32,
        connected_relays,
        queued_messages,
        capabilities: TransportCapabilities {
            supports_mesh: diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic.protocol, RelayProtocol::Mesh)),
            supports_paid_relays: seed
                .circles
                .iter()
                .any(|circle| matches!(circle.circle_type, CircleType::Paid)),
            supports_tor: input.use_tor_network,
            experimental_enabled: input.experimental_transport,
        },
        diagnostics,
        peers: cache.peers,
        session_sync: cache.session_sync,
        activities: cache.activities,
    }
}
