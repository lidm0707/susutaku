//! Reusable skill instruction blocks; attachable to agents via agent_skills.
//! Pure model — the SQL lives in the backend's postgres adapter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SkillRow {
    pub id: i64,
    pub name: String,
    pub body: String,
}

pub struct NewSkill<'a> {
    pub name: &'a str,
    pub body: &'a str,
}
