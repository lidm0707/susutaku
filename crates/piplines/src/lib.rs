//! Pipeline: named stages wired sequentially, payload passed stage to stage.
//! Pure data + ops, no I/O. Connectors to models/files live in callers.

pub mod agent;
pub mod graph;
pub mod payload;
pub mod port;
pub mod stage;

use crate::payload::Payload;
use crate::stage::{Stage, StageId};
use std::collections::VecDeque;

pub const MAX_STAGES: usize = 64;

#[derive(Debug)]
pub enum PipelineError {
    Empty,
    TooManyStages,
    StageFailed(StageId, String),
}

pub struct Pipeline {
    stages: VecDeque<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            stages: VecDeque::new(),
        }
    }

    pub fn attach(&mut self, stage: Box<dyn Stage>) -> Result<(), PipelineError> {
        if self.stages.len() >= MAX_STAGES {
            return Err(PipelineError::TooManyStages);
        }
        self.stages.push_back(stage);
        Ok(())
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub fn run(&mut self, mut payload: Payload) -> Result<Payload, PipelineError> {
        if self.stages.is_empty() {
            return Err(PipelineError::Empty);
        }
        while let Some(stage) = self.stages.pop_front() {
            let id = stage.id();
            payload = stage
                .apply(payload)
                .map_err(|e| PipelineError::StageFailed(id, e))?;
        }
        Ok(payload)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
