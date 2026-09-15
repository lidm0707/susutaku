use serde::{Deserialize, Serialize};

use crate::card::{Card, CardId};

pub const NO_SUCH_COLUMN: &str = "no such column";
pub const DUPLICATE_COLUMN: &str = "column id already exists";
pub const NO_SUCH_CARD: &str = "no such card";
pub const EMPTY_COLUMN: &str = "column is empty";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    NoSuchColumn(String),
    DuplicateColumn(String),
    NoSuchCard(CardId),
    EmptyColumn,
}

impl core::fmt::Display for BoardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BoardError::NoSuchColumn(id) => write!(f, "{NO_SUCH_COLUMN}: {id}"),
            BoardError::DuplicateColumn(id) => write!(f, "{DUPLICATE_COLUMN}: {id}"),
            BoardError::NoSuchCard(id) => write!(f, "{NO_SUCH_CARD}: {id}"),
            BoardError::EmptyColumn => write!(f, "{EMPTY_COLUMN}"),
        }
    }
}

impl std::error::Error for BoardError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub title: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Board {
    columns: Vec<Column>,
    next_card_id: CardId,
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_columns() -> Self {
        let mut board = Self::new();
        for (id, title) in crate::DEFAULT_COLUMNS {
            let _ = board.add_column(id, title);
        }
        board
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn add_column(&mut self, id: &str, title: &str) -> Result<(), BoardError> {
        if self.column_index(id).is_some() {
            return Err(BoardError::DuplicateColumn(id.to_string()));
        }
        self.columns.push(Column {
            id: id.to_string(),
            title: title.to_string(),
            cards: Vec::new(),
        });
        Ok(())
    }

    pub fn remove_column(&mut self, id: &str) -> Result<(), BoardError> {
        let idx = self.column_index(id).ok_or_else(|| BoardError::NoSuchColumn(id.to_string()))?;
        self.columns.remove(idx);
        Ok(())
    }

    pub fn add_card(&mut self, column_id: &str, title: &str) -> Result<CardId, BoardError> {
        let id = self.next_card_id;
        self.next_card_id += 1;
        let col = self.column_mut(column_id)?;
        col.cards.push(Card::new(id, title));
        Ok(id)
    }

    pub fn card(&self, id: CardId) -> Option<&Card> {
        self.columns
            .iter()
            .flat_map(|c| &c.cards)
            .find(|c| c.id == id)
    }

    pub fn card_mut(&mut self, id: CardId) -> Option<&mut Card> {
        self.columns
            .iter_mut()
            .flat_map(|c| &mut c.cards)
            .find(|c| c.id == id)
    }

    pub fn move_card(&mut self, id: CardId, to_column: &str, position: usize) -> Result<(), BoardError> {
        let (card, _) = self.take_card(id)?;
        let to_idx = self.column_index(to_column).ok_or_else(|| BoardError::NoSuchColumn(to_column.to_string()))?;
        let slot = position.min(self.columns[to_idx].cards.len());
        self.columns[to_idx].cards.insert(slot, card);
        Ok(())
    }

    pub fn remove_card(&mut self, id: CardId) -> Result<Card, BoardError> {
        self.take_card(id).map(|(card, _)| card)
    }

    pub fn column_of(&self, id: CardId) -> Option<&str> {
        self.columns
            .iter()
            .find(|c| c.cards.iter().any(|card| card.id == id))
            .map(|c| c.id.as_str())
    }

    fn column_index(&self, id: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.id == id)
    }

    fn column_mut(&mut self, id: &str) -> Result<&mut Column, BoardError> {
        let idx = self.column_index(id).ok_or_else(|| BoardError::NoSuchColumn(id.to_string()))?;
        self.columns.get_mut(idx).ok_or(BoardError::NoSuchColumn(id.to_string()))
    }

    fn take_card(&mut self, id: CardId) -> Result<(Card, usize), BoardError> {
        let from_idx = self
            .columns
            .iter()
            .position(|c| c.cards.iter().any(|c| c.id == id))
            .ok_or(BoardError::NoSuchCard(id))?;
        let col = &mut self.columns[from_idx];
        let pos = col.cards.iter().position(|c| c.id == id).ok_or(BoardError::NoSuchCard(id))?;
        Ok((col.cards.remove(pos), from_idx))
    }
}
