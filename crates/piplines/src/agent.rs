//! Agent-select node: picks which configured agent the pipeline uses.

use crate::payload::Payload;
use crate::stage::{Stage, StageId};

pub const PARAM_AGENT: &str = "agent";
pub const META_AGENT: &str = "agent";

#[derive(Clone, Debug, PartialEq)]
pub struct AgentNode {
    pub agent: String,
}

impl AgentNode {
    /// Build from a node's `params`, which must carry `{"agent": "<name>"}`.
    pub fn from_params(params: &serde_json::Value) -> Result<Self, String> {
        let agent = params
            .get(PARAM_AGENT)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("agent node params missing \"{}\"", PARAM_AGENT))?;
        Ok(Self {
            agent: agent.to_owned(),
        })
    }
}

impl Stage for AgentNode {
    fn id(&self) -> StageId {
        StageId::Agent
    }

    fn apply(&self, mut payload: Payload) -> Result<Payload, String> {
        payload.set_meta(META_AGENT, self.agent.clone());
        Ok(payload)
    }
}
