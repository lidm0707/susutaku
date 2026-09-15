//! Board operation value objects exchanged over the BoardOps port.

/// One board operation requested by the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardOp {
    /// Create a card in the default todo column; returns the new card id.
    CreateCard {
        project_id: i64,
        title: String,
        description: Option<String>,
    },
    /// Pin an agent to a card: the card runs this agent.
    AssignAgent { card_id: i64, agent: String },
    /// Set (or clear with None) the card's sandbox image.
    SetImage { card_id: i64, image: Option<String> },
    /// Set (or clear with None) a 5-field UTC cron on a card.
    SetCron { card_id: i64, cron: Option<String> },
    /// Run the card's assigned agent once, right now.
    RunCard { card_id: i64 },
    /// Projects/cards summary with ids.
    Summary,
    /// Cards whose title or description contains the query (case-insensitive).
    FindCards { query: String },
}

/// A board operation plus the caller's bearer token (authorization input).
#[derive(Debug, Clone)]
pub struct BoardRequest {
    pub token: Option<String>,
    pub op: BoardOp,
}

/// Result of a board operation, formatted for the tool-result context.
pub type BoardResult = Result<String, String>;
