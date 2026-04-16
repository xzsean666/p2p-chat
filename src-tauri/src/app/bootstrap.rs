use crate::domain::bootstrap::BootstrapStatus;

pub fn build_bootstrap_status() -> BootstrapStatus {
    BootstrapStatus {
        project: "P2P Chat",
        phase: "Transport Foundation",
        ready: true,
        stack: vec!["Rust", "Tauri 2", "Vue 3", "TypeScript", "PrimeVue"],
        next: vec![
            "Route and shell scaffolding",
            "Transport mutation pipeline",
            "Peer discovery and session sync",
            "Persistence and packaging",
        ],
    }
}
