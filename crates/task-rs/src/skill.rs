//! Reusable skill instruction blocks; attachable to agents via agent_skills.

use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

const UNIQUE_VIOLATION: &str = "23505";
const FK_VIOLATION: &str = "23503";

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

impl Store {
    pub async fn list_skills(&self) -> Result<Vec<SkillRow>, StoreError> {
        let rows = sqlx::query_as!(
            SkillRow,
            r#"SELECT id AS "id: i64", name, body FROM skills ORDER BY id"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn create_skill(&self, skill: NewSkill<'_>) -> Result<SkillRow, StoreError> {
        let row = sqlx::query!(
            r#"INSERT INTO skills (name, body) VALUES ($1, $2)
               RETURNING id AS "id: i64""#,
            skill.name,
            skill.body,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(UNIQUE_VIOLATION) => StoreError::SkillTaken,
            _ => StoreError::Db(e),
        })?;
        Ok(SkillRow {
            id: row.id,
            name: skill.name.to_string(),
            body: skill.body.to_string(),
        })
    }

    pub async fn remove_skill(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"DELETE FROM skills WHERE id = $1"#, id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchSkill);
        }
        Ok(())
    }

    pub async fn update_skill(&self, id: i64, body: &str) -> Result<(), StoreError> {
        let res = sqlx::query!(r#"UPDATE skills SET body = $2 WHERE id = $1"#, id, body)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchSkill);
        }
        Ok(())
    }

    pub async fn list_agent_skills(&self, agent_id: i64) -> Result<Vec<SkillRow>, StoreError> {
        let rows = sqlx::query_as!(
            SkillRow,
            r#"SELECT s.id AS "id: i64", s.name, s.body
               FROM skills s
               JOIN agent_skills a ON a.skill_id = s.id
               WHERE a.agent_id = $1
               ORDER BY s.id"#,
            agent_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn attach_agent_skill(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        sqlx::query!(
            r#"INSERT INTO agent_skills (agent_id, skill_id) VALUES ($1, $2)
               ON CONFLICT DO NOTHING"#,
            agent_id,
            skill_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e.as_database_error() {
            Some(d) if d.code().as_deref() == Some(FK_VIOLATION) => StoreError::NoSuchSkill,
            _ => StoreError::Db(e),
        })?;
        Ok(())
    }

    pub async fn detach_agent_skill(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!(
            r#"DELETE FROM agent_skills WHERE agent_id = $1 AND skill_id = $2"#,
            agent_id,
            skill_id,
        )
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchSkill);
        }
        Ok(())
    }
}
