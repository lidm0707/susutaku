//! Local-model endpoint persisted in the repo-root `setting.json` under
//! the `local` section.

pub const LOCAL_SECTION: &str = "local";
pub const FIELD_ENDPOINT: &str = "endpoint";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalSettings {
    pub endpoint: String,
}

pub fn read_saved() -> Option<LocalSettings> {
    read(&super::zai_settings::read_doc())
}

pub fn read(doc: &serde_json::Value) -> Option<LocalSettings> {
    Some(LocalSettings {
        endpoint: doc
            .get(LOCAL_SECTION)?
            .get(FIELD_ENDPOINT)?
            .as_str()?
            .to_owned(),
    })
}

pub fn write(doc: &mut serde_json::Value, endpoint: &str) {
    doc[LOCAL_SECTION] = serde_json::json!({ FIELD_ENDPOINT: endpoint });
}
