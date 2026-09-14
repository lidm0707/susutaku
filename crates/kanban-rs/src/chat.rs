//! Chat transcript persistence: threads + messages, sqlx `query_as!` only.

use crate::store::StoreError;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ChatThreadRow {
    pub id: i64,
    pub agent: String,
    pub title: String,
    pub project_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ChatMessageRow {
    pub id: i64,
    pub thread_id: i64,
    pub role: String,
    pub text: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub const ROLE_USER: &str = "user";
pub const ROLE_ASSISTANT: &str = "assistant";

impl crate::store::Store {
    pub async fn create_chat_thread(
        &self,
        agent: &str,
        title: &str,
        project_id: Option<i64>,
    ) -> Result<ChatThreadRow, StoreError> {
        let row = sqlx::query_as!(
            ChatThreadRow,
            r#"INSERT INTO chat_threads (agent, title, project_id) VALUES ($1, $2, $3)
               RETURNING id, agent, title, project_id,
                         created_at,
                         created_at AS "updated_at!""#,
            agent,
            title,
            project_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn chat_thread(&self, id: i64) -> Result<Option<ChatThreadRow>, StoreError> {
        sqlx::query_as!(
            ChatThreadRow,
            r#"SELECT id, agent, title, project_id, created_at,
                      created_at AS "updated_at!"
               FROM chat_threads
               WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Threads scoped to one project; `None` lists project-less threads.
    pub async fn list_chat_threads(
        &self,
        project_id: Option<i64>,
    ) -> Result<Vec<ChatThreadRow>, StoreError> {
        let rows = sqlx::query_as!(
            ChatThreadRow,
            r#"SELECT t.id, t.agent, t.title, t.project_id, t.created_at,
                      COALESCE(MAX(m.created_at), t.created_at) AS "updated_at!"
               FROM chat_threads t
               LEFT JOIN chat_messages m ON m.thread_id = t.id
               WHERE t.project_id IS NOT DISTINCT FROM $1
               GROUP BY t.id
               ORDER BY COALESCE(MAX(m.created_at), t.created_at) DESC, t.id DESC
               LIMIT 200"#,
            project_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_chat_thread(&self, id: i64) -> Result<(), StoreError> {
        let res = sqlx::query!("DELETE FROM chat_threads WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NoSuchChatThread);
        }
        Ok(())
    }

    pub async fn add_chat_message(
        &self,
        thread_id: i64,
        role: &str,
        text: &str,
    ) -> Result<ChatMessageRow, StoreError> {
        let row = sqlx::query_as!(
            ChatMessageRow,
            r#"INSERT INTO chat_messages (thread_id, role, text) VALUES ($1, $2, $3)
               RETURNING id, thread_id, role, text, created_at"#,
            thread_id,
            role,
            text
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_chat_messages(
        &self,
        thread_id: i64,
    ) -> Result<Vec<ChatMessageRow>, StoreError> {
        let rows = sqlx::query_as!(
            ChatMessageRow,
            r#"SELECT id, thread_id, role, text, created_at FROM chat_messages WHERE thread_id = $1 ORDER BY id"#,
            thread_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
