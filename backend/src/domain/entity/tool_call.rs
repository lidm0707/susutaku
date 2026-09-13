//! Entity: a tool invocation parsed out of a model reply. It is the object
//! the chat use case acts upon in the agentic loop.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Search(String),
    Fetch(String),
    Shell(String),
    CardCreate { project_id: i64, title: String },
    CardSchedule { card_id: i64, cron: String },
    CardLink { card_id: i64, pipeline_id: i64 },
    PipelineCreate { name: String, spec: Option<String> },
    BoardList,
}

const TOOL_PREFIX: &str = "TOOL:";
const INVOKE_OPEN: &str = "<invoke";
const NAME_ATTR: &str = "name=";
const PARAM_CLOSE: &str = "</parameter>";
const XML_SEARCH: &str = "search";
const XML_FETCH: &str = "fetch";
const XML_SHELL: &str = "shell";
const XML_CARD_CREATE: &str = "card_create";
const XML_CARD_ROUTINE: &str = "card_routine";
const XML_CARD_LINK: &str = "card_link";
const XML_PIPELINE_CREATE: &str = "pipeline_create";
const XML_BOARD_LIST: &str = "board_list";
const TOOL_SEARCH: &str = "SEARCH";
const TOOL_FETCH: &str = "FETCH";
const TOOL_SHELL: &str = "SHELL";
const TOOL_CARD_CREATE: &str = "CARD_CREATE";
const TOOL_CARD_ROUTINE: &str = "CARD_ROUTINE";
const TOOL_CARD_LINK: &str = "CARD_LINK";
const TOOL_PIPELINE_CREATE: &str = "PIPELINE_CREATE";
const TOOL_BOARD_LIST: &str = "BOARD_LIST";

impl ToolCall {
    /// First TOOL: line after the </think> block, if any. The argument may be
    /// empty (e.g. `TOOL: BOARD_LIST`).
    pub fn parse(reply: &str) -> Option<Self> {
        let visible = reply
            .split_once("</think>")
            .map(|(_, after)| after)
            .unwrap_or(reply);
        if let Some(call) = Self::parse_xml(visible) {
            return Some(call);
        }
        let line = visible
            .lines()
            .map(str::trim_start)
            .find(|l| l.to_uppercase().starts_with(TOOL_PREFIX))?;
        let rest = line[TOOL_PREFIX.len()..].trim();
        let (kind, arg) = match rest.split_once(' ') {
            Some((kind, arg)) => (kind, arg.trim()),
            None => (rest, ""),
        };
        match kind.to_uppercase().as_str() {
            TOOL_SEARCH => Some(Self::Search(arg.to_string())),
            TOOL_FETCH => Some(Self::Fetch(arg.to_string())),
            TOOL_SHELL => Some(Self::Shell(arg.to_string())),
            TOOL_CARD_CREATE => {
                let (project_id, title) = arg.split_once(' ')?;
                Some(Self::CardCreate {
                    project_id: project_id.parse().ok()?,
                    title: title.to_string(),
                })
            }
            TOOL_CARD_ROUTINE => {
                let (card_id, cron) = arg.split_once(' ')?;
                Some(Self::CardSchedule {
                    card_id: card_id.parse().ok()?,
                    cron: cron.to_string(),
                })
            }
            TOOL_CARD_LINK => {
                let (card_id, pipeline_id) = arg.split_once(' ')?;
                Some(Self::CardLink {
                    card_id: card_id.parse().ok()?,
                    pipeline_id: pipeline_id.parse().ok()?,
                })
            }
            TOOL_PIPELINE_CREATE => {
                let (name, spec) = match arg.split_once(' ') {
                    Some((name, spec)) => (name, Some(spec.trim())),
                    None => (arg, None),
                };
                if name.is_empty() {
                    return None;
                }
                let spec = spec.map(str::trim).filter(|s| !s.is_empty());
                Some(Self::PipelineCreate {
                    name: name.to_string(),
                    spec: spec.map(str::to_string),
                })
            }
            TOOL_BOARD_LIST => Some(Self::BoardList),
            _ => None,
        }
    }

    /// Fallback for models that answer with Anthropic-style
    /// `<invoke name="x"><parameter name="k">v</parameter></invoke>` blocks
    /// instead of the TOOL: line protocol.
    fn parse_xml(reply: &str) -> Option<Self> {
        let open = reply.split_once(INVOKE_OPEN)?.1;
        let (attrs, body) = open.split_once('>')?;
        let name = attr_value(attrs, NAME_ATTR)?.to_lowercase();
        let p = |key: &str| param_value(body, key);
        match name.as_str() {
            XML_SEARCH => Some(Self::Search(p("query")?.to_string())),
            XML_FETCH => Some(Self::Fetch(p("url")?.to_string())),
            XML_SHELL => Some(Self::Shell(p("command")?.to_string())),
            XML_CARD_CREATE => Some(Self::CardCreate {
                project_id: p("project_id")?.trim().parse().ok()?,
                title: p("title")?.to_string(),
            }),
            XML_CARD_ROUTINE => Some(Self::CardSchedule {
                card_id: p("card_id")?.trim().parse().ok()?,
                cron: p("cron")?.to_string(),
            }),
            XML_CARD_LINK => Some(Self::CardLink {
                card_id: p("card_id")?.trim().parse().ok()?,
                pipeline_id: p("pipeline_id")?.trim().parse().ok()?,
            }),
            XML_PIPELINE_CREATE => {
                let name = p("name")?;
                if name.is_empty() {
                    return None;
                }
                let spec = p("spec")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Some(Self::PipelineCreate {
                    name: name.to_string(),
                    spec,
                })
            }
            XML_BOARD_LIST => Some(Self::BoardList),
            _ => None,
        }
    }
}

fn attr_value<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let rest = attrs.split_once(key)?.1.strip_prefix('"')?;
    Some(rest.split_once('"')?.0)
}

fn param_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let tag = format!("<parameter name=\"{key}\">");
    let rest = body.split_once(tag.as_str())?.1;
    Some(rest.split_once(PARAM_CLOSE)?.0.trim())
}
