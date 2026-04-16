use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub project: &'static str,
    pub phase: &'static str,
    pub ready: bool,
    pub stack: Vec<&'static str>,
    pub next: Vec<&'static str>,
}
