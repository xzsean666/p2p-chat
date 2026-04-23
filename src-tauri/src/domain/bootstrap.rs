use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub project: &'static str,
    pub phase: &'static str,
    // `ready` must stay conservative while transport is still preview/experimental.
    pub ready: bool,
    pub stack: Vec<&'static str>,
    pub next: Vec<&'static str>,
}
