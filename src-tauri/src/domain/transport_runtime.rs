use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;

pub trait TransportRuntime {
    fn build_profiles(
        &self,
        seed: &ChatDomainSeed,
        options: TransportRuntimeOptions,
    ) -> Result<Vec<TransportRuntimeProfile>, String>;
}
