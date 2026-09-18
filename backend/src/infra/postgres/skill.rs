//! Postgres adapter for the `skills` + `agent_skills` tables.

use async_trait::async_trait;
use task_rs::{NewSkill, SkillRow, StoreError};

use super::{PgTask, Store};
use crate::port::outbound::SkillRepo;

const UNIQUE_VIOLATION: &str = "23505";
const FK_VIOLATION: &str = "23503";

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

#[async_trait]
impl SkillRepo for PgTask {
    async fn list(&self) -> Result<Vec<SkillRow>, StoreError> {
        self.store().list_skills().await
    }

    async fn create(&self, name: &str, body: &str) -> Result<SkillRow, StoreError> {
        self.store().create_skill(NewSkill { name, body }).await
    }

    async fn update(&self, id: i64, body: &str) -> Result<(), StoreError> {
        self.store().update_skill(id, body).await
    }

    async fn remove(&self, id: i64) -> Result<(), StoreError> {
        self.store().remove_skill(id).await
    }

    async fn list_for_agent(&self, agent_id: i64) -> Result<Vec<SkillRow>, StoreError> {
        self.store().list_agent_skills(agent_id).await
    }

    async fn attach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.store().attach_agent_skill(agent_id, skill_id).await
    }

    async fn detach(&self, agent_id: i64, skill_id: i64) -> Result<(), StoreError> {
        self.store().detach_agent_skill(agent_id, skill_id).await
    }
}
