//! Live-DB test: the project-skill seeder. Seeds the skill, attaches it to a
//! fresh agent, and is idempotent (a second run attaches nothing new).

use std::sync::Arc;

use backend::api::{PROJECT_SKILL_NAME, seed_project_skill};
use backend::infra::postgres::Store;
use task_rs::AgentConfigRow;

fn unique_name(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{tag}_{nanos}")
}

fn agent_cfg(name: String) -> AgentConfigRow {
    AgentConfigRow {
        id: 0,
        name,
        model: "model".into(),
        persona: String::new(),
        prompt: String::new(),
        output: String::new(),
        allowed_tools: vec!["board".into()],
        receive_images: false,
        thinking: "off".into(),
        ctx_limit: 128000,
        ctx_policy: "compact".into(),
    }
}

#[tokio::test]
async fn project_skill_is_seeded_and_attached() {
    let store = Arc::new(Store::connect(Store::default_url()).await.expect("connect"));

    seed_project_skill(&store).await;
    let skills = store.list_skills().await.expect("skills");
    let skill = skills
        .iter()
        .find(|s| s.name == PROJECT_SKILL_NAME)
        .expect("project skill seeded");
    assert!(!skill.body.is_empty());
    assert!(!skill.body.starts_with("---"), "frontmatter stripped");
    assert!(skill.body.contains("CARD_FIND"));

    // A fresh agent must get the skill on the next seed run.
    let agent_id = store
        .create_agent(&agent_cfg(unique_name("skill-agent")))
        .await
        .expect("agent");
    seed_project_skill(&store).await;
    let attached = store
        .list_agent_skills(agent_id)
        .await
        .expect("agent skills");
    assert!(attached.iter().any(|s| s.id == skill.id));

    // Idempotent: a second run does not duplicate the attachment.
    seed_project_skill(&store).await;
    let attached = store
        .list_agent_skills(agent_id)
        .await
        .expect("agent skills");
    assert_eq!(attached.iter().filter(|s| s.id == skill.id).count(), 1);

    let _ = store.remove_agent(agent_id).await;
}
