//! Business state; mutations must run under one application state lock.
//! Callers must validate and uniquely correlate responses before applying them.
mod calls;
mod devices;
pub use calls::*;
pub use devices::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type DomainResult<T> = Result<T, String>;
pub(crate) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainStore {
    pub devices: BTreeMap<String, Device>,
    pub sessions: BTreeMap<String, Session>,
    pub calls: BTreeMap<String, Call>,
    pub resources: BTreeMap<String, Resource>,
    pub pools: BTreeMap<String, ResourcePool>,
    #[serde(default)]
    pub config_history: Vec<ConfigOutcome>,
    #[serde(default)]
    pub(crate) applied_requests: BTreeMap<String, ConfigOutcome>,
}
