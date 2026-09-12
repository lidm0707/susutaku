//! Board operation value objects exchanged over the BoardOps port.

/// One board operation requested by the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardOp {
    /// Create a pipeline; the spec is the optional graph JSON
    /// `{nodes: [{id, stage, params}], links: [{from, to}]}` — without it an
    /// empty (valid) pipeline is created. Returns the new pipeline id.
    CreatePipeline { name: String, spec: Option<String> },
    /// Create a card in the default todo column; returns the new card id.
    CreateCard { project_id: i64, title: String },
    /// Attach a pipeline to a card.
    LinkPipeline { card_id: i64, pipeline_id: i64 },
    /// Set (or clear with None) a 5-field UTC cron on a card.
    SetCron { card_id: i64, cron: Option<String> },
    /// Projects/pipelines/cards summary with ids.
    Summary,
}

/// A board operation plus the caller's bearer token (authorization input).
#[derive(Debug, Clone)]
pub struct BoardRequest {
    pub token: Option<String>,
    pub op: BoardOp,
}

/// Result of a board operation, formatted for the tool-result context.
pub type BoardResult = Result<String, String>;
