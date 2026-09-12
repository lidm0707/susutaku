//! Integration test: skill CRUD + agent attachment.

use kanban_rs::{NewSkill, Store};

const SKILL_PREFIX: &str = "test_skill_";

async fn cleanup(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM skills WHERE name LIKE $1")
        .bind(format!("{SKILL_PREFIX}%"))
        .execute(pool)
        .await
        .expect("cleanup");
    sqlx::query("DELETE FROM agent_settings WHERE name LIKE $1")
        .bind(format!("{SKILL_PREFIX}%"))
        .execute(pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn skill_crud_and_attach() {
    let store = Store::connect(Store::default_url()).await.expect("connect");
    let pool = sqlx::PgPool::connect(Store::default_url())
        .await
        .expect("pool");
    cleanup(&pool).await;

    let skill = store
        .create_skill(NewSkill {
            name: &format!("{SKILL_PREFIX}rust"),
            body: "write lean rust",
        })
        .await
        .expect("create");

    let updated = store.update_skill(skill.id, "write lean rust, zero copy");
    assert!(updated.await.is_ok());

    let agent = sqlx::query!(
        r#"INSERT INTO agent_settings (name) VALUES ($1) RETURNING id AS "id: i64""#,
        format!("{SKILL_PREFIX}agent"),
    )
    .fetch_one(&pool)
    .await
    .expect("agent");

    store
        .attach_agent_skill(agent.id, skill.id)
        .await
        .expect("attach");
    store
        .attach_agent_skill(agent.id, skill.id)
        .await
        .expect("re-attach is idempotent");

    let attached = store.list_agent_skills(agent.id).await.expect("list");
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0].body, "write lean rust, zero copy");

    store
        .detach_agent_skill(agent.id, skill.id)
        .await
        .expect("detach");
    assert!(
        store
            .list_agent_skills(agent.id)
            .await
            .expect("list")
            .is_empty()
    );

    assert!(store.remove_skill(skill.id).await.is_ok());
    assert!(store.remove_skill(skill.id).await.is_err());
    cleanup(&pool).await;
}
