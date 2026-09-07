//! Stage trait + stage ids. Implement `apply` to transform a payload.

use crate::payload::Payload;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageId {
    Ingest,
    Parse,
    Transform,
    ModelInfer,
    Render,
    Fetch,
    Search,
    RefImage,
    OutputResource,
    Agent,
    Raw,
    Custom(&'static str),
}

pub trait Stage {
    fn id(&self) -> StageId;
    fn apply(&self, payload: Payload) -> Result<Payload, String>;
}
