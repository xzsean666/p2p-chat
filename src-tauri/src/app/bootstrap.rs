use crate::domain::bootstrap::BootstrapStatus;

pub fn build_bootstrap_status() -> BootstrapStatus {
    BootstrapStatus {
        project: "P2P Chat",
        phase: "Transport Preview",
        ready: false,
        stack: vec!["Rust", "Tauri 2", "Vue 3", "TypeScript", "PrimeVue"],
        next: vec![
            "Harden preview transport diagnostics and launch reporting",
            "Replace optimistic preview states with verified runtime readiness",
            "Complete remote group transport and session sync flows",
            "Promote preview runtime paths only after end-to-end validation",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::build_bootstrap_status;

    #[test]
    fn build_bootstrap_status_marks_transport_as_preview_not_ready() {
        let status = build_bootstrap_status();

        assert_eq!(status.project, "P2P Chat");
        assert_eq!(status.phase, "Transport Preview");
        assert!(!status.ready);
        assert!(status
            .next
            .iter()
            .any(|item| item.contains("verified runtime readiness")));
    }
}
