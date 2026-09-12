//! Declared per-stage port kinds + param schema: what a node consumes and emits.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    Any,
    Text,
    Json,
    Image,
}

pub const KEY_INPUT: &str = "input";
pub const KEY_OUTPUT: &str = "output";
pub const KEY_WIRED: &str = "wired";
pub const KEY_DOC: &str = "doc";
pub const KEY_PARAMS: &str = "params";
pub const KEY_KEY: &str = "key";
pub const KEY_HINT: &str = "hint";
pub const KEY_REQUIRED: &str = "required";

impl std::fmt::Display for PortKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PortKind::Any => "any",
            PortKind::Text => "text",
            PortKind::Json => "json",
            PortKind::Image => "image",
        };
        f.write_str(name)
    }
}

pub fn ports(stage: &str) -> (PortKind, PortKind) {
    match stage {
        crate::graph::STAGE_PARSE => (PortKind::Text, PortKind::Json),
        crate::graph::STAGE_TRANSFORM => (PortKind::Text, PortKind::Text),
        crate::graph::STAGE_MODEL_INFER => (PortKind::Any, PortKind::Text),
        crate::graph::STAGE_FETCH => (PortKind::Any, PortKind::Text),
        crate::graph::STAGE_SEARCH => (PortKind::Any, PortKind::Text),
        crate::graph::STAGE_REF_IMAGE => (PortKind::Any, PortKind::Image),
        _ => (PortKind::Any, PortKind::Any),
    }
}

/// Stages wired to a runtime engine in backend `pipeline_run.rs`. Unwired
/// stages fail at run time, so the UI must not offer them as new nodes.
pub const WIRED_STAGES: [&str; 6] = [
    crate::graph::STAGE_FETCH,
    crate::graph::STAGE_AGENT,
    crate::graph::STAGE_OUTPUT_RESOURCE,
    crate::graph::STAGE_TRANSFORM,
    crate::graph::STAGE_SEARCH,
    crate::graph::STAGE_REF_IMAGE,
];

pub fn wired(stage: &str) -> bool {
    WIRED_STAGES.contains(&stage)
}

/// One editable param of a stage, shown as a typed field in the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamSpec {
    pub key: &'static str,
    pub hint: &'static str,
    pub required: bool,
}

pub const PARAM_URL: ParamSpec = ParamSpec {
    key: "url",
    hint: "http(s) url to fetch",
    required: true,
};
pub const PARAM_METHOD: ParamSpec = ParamSpec {
    key: "method",
    hint: "http method, default GET",
    required: false,
};
pub const PARAM_OP: ParamSpec = ParamSpec {
    key: "op",
    hint: "upper | lower | trim",
    required: true,
};
pub const PARAM_AGENT: ParamSpec = ParamSpec {
    key: "agent",
    hint: "agent name from Agents settings",
    required: true,
};
pub const PARAM_NAME: ParamSpec = ParamSpec {
    key: "name",
    hint: "resource name to write result to",
    required: true,
};
pub const PARAM_QUERY: ParamSpec = ParamSpec {
    key: "query",
    hint: "web search query (empty: use incoming text)",
    required: false,
};
pub const PARAM_PATH: ParamSpec = ParamSpec {
    key: "path",
    hint: "uploaded image path",
    required: true,
};

/// Everything the UI needs to render one node kind honestly.
#[derive(Clone, Debug, PartialEq)]
pub struct StageSchema {
    pub input: PortKind,
    pub output: PortKind,
    pub wired: bool,
    pub doc: &'static str,
    pub params: &'static [ParamSpec],
}

const DOC_FETCH: &str = "downloads the resource at url and emits the raw body as text downstream. usually the first node.";
const DOC_AGENT: &str = "selects the configured agent for downstream nodes. place before the node that calls the model.";
const DOC_OUTPUT: &str = "writes the incoming text payload to the resource named name. use as the last node of a branch; it passes the payload through.";
const DOC_TRANSFORM: &str = "reshapes the text payload: upper, lower or trim.";
const DOC_SEARCH: &str = "runs the web search query and emits the results as text; empty query searches the incoming text.";
const DOC_REF_IMAGE: &str = "loads the uploaded image at path into the payload as binary; place before a node that reads images.";
const DOC_UNWIRED: &str =
    "not wired to an engine yet: fails at run time. kept for old saved specs.";

const SCHEMA_REQUIRED_TAG: &str = "required";
const SCHEMA_OPTIONAL_TAG: &str = "optional";
const SCHEMA_NO_PARAMS: &str = "no params";

/// One line per runnable (wired) stage, for model prompts: the doc plus its
/// params with required/optional tags. Unwired stages are omitted: they fail
/// at run time, so a model must never emit them.
pub fn schema_text() -> String {
    let mut lines: Vec<String> = Vec::new();
    for stage in WIRED_STAGES {
        let s = stage_schema(stage);
        let params = if s.params.is_empty() {
            SCHEMA_NO_PARAMS.to_owned()
        } else {
            let items = s
                .params
                .iter()
                .map(|p| {
                    let tag = if p.required {
                        SCHEMA_REQUIRED_TAG
                    } else {
                        SCHEMA_OPTIONAL_TAG
                    };
                    format!("{} ({}: {})", p.key, tag, p.hint)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("params: {items}")
        };
        lines.push(format!("{stage} — {}; {params}", s.doc));
    }
    lines.join("\n")
}

pub fn stage_schema(stage: &str) -> StageSchema {
    let (input, output) = ports(stage);
    let params: &[ParamSpec] = match stage {
        crate::graph::STAGE_FETCH => &[PARAM_URL, PARAM_METHOD],
        crate::graph::STAGE_AGENT => &[PARAM_AGENT],
        crate::graph::STAGE_OUTPUT_RESOURCE => &[PARAM_NAME],
        crate::graph::STAGE_TRANSFORM => &[PARAM_OP],
        crate::graph::STAGE_SEARCH => &[PARAM_QUERY],
        crate::graph::STAGE_REF_IMAGE => &[PARAM_PATH],
        _ => &[],
    };
    let doc: &str = match stage {
        crate::graph::STAGE_FETCH => DOC_FETCH,
        crate::graph::STAGE_AGENT => DOC_AGENT,
        crate::graph::STAGE_OUTPUT_RESOURCE => DOC_OUTPUT,
        crate::graph::STAGE_TRANSFORM => DOC_TRANSFORM,
        crate::graph::STAGE_SEARCH => DOC_SEARCH,
        crate::graph::STAGE_REF_IMAGE => DOC_REF_IMAGE,
        _ => DOC_UNWIRED,
    };
    StageSchema {
        input,
        output,
        wired: wired(stage),
        doc,
        params,
    }
}

/// `Any` accepts everything; identical kinds always match.
pub fn compatible(out: PortKind, input: PortKind) -> bool {
    match (out, input) {
        (PortKind::Any, _) | (_, PortKind::Any) => true,
        (a, b) => a == b,
    }
}

/// Stage -> schema map for the UI schema endpoint.
pub fn schema() -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for stage in crate::graph::STAGE_NAMES {
        let s = stage_schema(stage);
        let params: Vec<serde_json::Value> = s
            .params
            .iter()
            .map(|p| {
                serde_json::json!({
                    KEY_KEY: p.key,
                    KEY_HINT: p.hint,
                    KEY_REQUIRED: p.required,
                })
            })
            .collect();
        map.insert(
            stage.to_owned(),
            serde_json::json!({
                KEY_INPUT: s.input,
                KEY_OUTPUT: s.output,
                KEY_WIRED: s.wired,
                KEY_DOC: s.doc,
                KEY_PARAMS: params,
            }),
        );
    }
    serde_json::Value::Object(map)
}
