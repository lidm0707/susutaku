//! Entity: a tool invocation parsed out of a model reply. It is the object
//! the chat use case acts upon in the agentic loop.

use crate::domain::GitOp;

/// Language-server query the LSP tool performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspOp {
    Definition,
    References,
    Hover,
}

impl LspOp {
    const DEFINITION: &str = "DEFINITION";
    const REFERENCES: &str = "REFERENCES";
    const HOVER: &str = "HOVER";

    fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            Self::DEFINITION => Some(Self::Definition),
            Self::REFERENCES => Some(Self::References),
            Self::HOVER => Some(Self::Hover),
            _ => None,
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Definition => Self::DEFINITION,
            Self::References => Self::REFERENCES,
            Self::Hover => Self::HOVER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Search(String),
    Fetch(String),
    Shell(String),
    Coding {
        path: String,
        code: String,
    },
    CardCreate {
        project_id: i64,
        title: String,
        description: Option<String>,
    },
    CardSchedule {
        card_id: i64,
        cron: String,
    },
    CardRoutineClear {
        card_id: i64,
    },
    CardRun {
        card_id: i64,
    },
    CardAgent {
        card_id: i64,
        agent: String,
    },
    CardImage {
        card_id: i64,
        image: Option<String>,
    },
    BoardList,
    CardFind {
        query: String,
    },
    Math(String),
    Git {
        op: GitOp,
        /// Agent whose sandbox runs the op (`@agent` suffix / `agent` param).
        agent: Option<String>,
    },
    Lsp {
        op: LspOp,
        /// Workspace-relative file path.
        path: String,
        /// 0-based line of the symbol position.
        line: u32,
        /// 0-based UTF-8 column of the symbol position.
        col: usize,
    },
    AgentRun {
        agent: String,
        cmd: String,
    },
}

const TOOL_PREFIX: &str = "TOOL:";
const DESC_SEP: &str = " | ";
const INVOKE_OPEN: &str = "<invoke";
const NAME_ATTR: &str = "name=";
const PARAM_TITLE: &str = "title";
const PARAM_TITLE_ALIAS: &str = "name";
const PARAM_CLOSE: &str = "</parameter>";
const XML_SEARCH: &str = "search";
const XML_FETCH: &str = "fetch";
const XML_SHELL: &str = "shell";
const XML_CODING: &str = "coding";
const XML_CODING_ALIAS: &str = "write_file";
const XML_CARD_CREATE: &str = "card_create";
const XML_CARD_CREATE_ALIAS: &str = "create_card";
const XML_CARD_ROUTINE: &str = "card_routine";
const XML_CARD_ROUTINE_CLEAR: &str = "card_routine_clear";
const XML_CARD_RUN: &str = "card_run";
const XML_CARD_AGENT: &str = "card_agent";
const XML_CARD_IMAGE: &str = "card_image";
const XML_BOARD_LIST: &str = "board_list";
const XML_CARD_FIND: &str = "card_find";
const XML_CARD_FIND_ALIAS: &str = "find_card";
const XML_MATH: &str = "math";
const XML_MATH_ALIAS: &str = "geomath";
const XML_GIT: &str = "git";
const XML_LSP: &str = "lsp";
const XML_AGENT_RUN: &str = "agent_run";
const TOOL_SEARCH: &str = "SEARCH";
const TOOL_FETCH: &str = "FETCH";
const TOOL_SHELL: &str = "SHELL";
const TOOL_CARD_CREATE: &str = "CARD_CREATE";
const TOOL_CARD_ROUTINE: &str = "CARD_ROUTINE";
const TOOL_CARD_ROUTINE_CLEAR: &str = "CARD_ROUTINE_CLEAR";
const TOOL_CARD_RUN: &str = "CARD_RUN";
const TOOL_CARD_AGENT: &str = "CARD_AGENT";
const TOOL_CARD_IMAGE: &str = "CARD_IMAGE";
const TOOL_BOARD_LIST: &str = "BOARD_LIST";
const TOOL_CARD_FIND: &str = "CARD_FIND";
const TOOL_MATH: &str = "MATH";
const TOOL_MATH_ALIAS: &str = "GEOMATH";
const TOOL_GIT: &str = "GIT";
const TOOL_LSP: &str = "LSP";
const TOOL_AGENT_RUN: &str = "AGENT_RUN";
const LSP_ARGS: usize = 4;
const GIT_OP_CLONE: &str = "CLONE";
const AGENT_PREFIX: &str = "@";
const CRON_CLEAR: &str = "clear";
const GIT_OP_STATUS: &str = "STATUS";
const GIT_OP_DIFF: &str = "DIFF";
const GIT_OP_BRANCH: &str = "BRANCH";
const GIT_OP_COMMIT: &str = "COMMIT";
const GIT_OP_PUSH: &str = "PUSH";
const GIT_OP_PR: &str = "PR";
const PR_ARG_SEP: char = '|';

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
                let (project_id, rest) = arg.split_once(' ')?;
                let (title, description) = match rest.split_once(DESC_SEP) {
                    Some((t, d)) => (t.trim(), Some(d.trim().to_string())),
                    None => (rest, None),
                };
                if title.is_empty() {
                    return None;
                }
                Some(Self::CardCreate {
                    project_id: project_id.parse().ok()?,
                    title: title.to_string(),
                    description,
                })
            }
            TOOL_CARD_ROUTINE => {
                let (card_id, cron) = arg.split_once(' ')?;
                if cron.eq_ignore_ascii_case(CRON_CLEAR) {
                    return Some(Self::CardRoutineClear {
                        card_id: card_id.parse().ok()?,
                    });
                }
                Some(Self::CardSchedule {
                    card_id: card_id.parse().ok()?,
                    cron: cron.to_string(),
                })
            }
            TOOL_CARD_ROUTINE_CLEAR => Some(Self::CardRoutineClear {
                card_id: arg.parse().ok()?,
            }),
            TOOL_CARD_RUN => Some(Self::CardRun {
                card_id: arg.parse().ok()?,
            }),
            TOOL_CARD_AGENT => {
                let (card_id, agent) = arg.split_once(' ')?;
                let agent = agent.trim();
                if agent.is_empty() {
                    return None;
                }
                Some(Self::CardAgent {
                    card_id: card_id.parse().ok()?,
                    agent: agent.to_string(),
                })
            }
            TOOL_CARD_IMAGE => {
                let (card_id, image) = arg.split_once(' ')?;
                let image = image.trim();
                let clear = image.eq_ignore_ascii_case(CRON_CLEAR);
                if image.is_empty() && !clear {
                    return None;
                }
                Some(Self::CardImage {
                    card_id: card_id.parse().ok()?,
                    image: (!clear).then(|| image.to_string()),
                })
            }
            TOOL_BOARD_LIST => Some(Self::BoardList),
            TOOL_CARD_FIND if !arg.is_empty() => Some(Self::CardFind {
                query: arg.to_string(),
            }),
            TOOL_MATH | TOOL_MATH_ALIAS if !arg.is_empty() => Some(Self::Math(arg.to_string())),
            TOOL_GIT => Self::parse_git(arg),
            TOOL_LSP => Self::parse_lsp(arg),
            TOOL_AGENT_RUN => Self::parse_agent_run(arg),
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
            XML_CODING | XML_CODING_ALIAS => {
                let path = p("path")?.trim().to_string();
                if path.is_empty() {
                    return None;
                }
                Some(Self::Coding {
                    path,
                    code: p("code")?.to_string(),
                })
            }
            XML_CARD_CREATE | XML_CARD_CREATE_ALIAS => Some(Self::CardCreate {
                project_id: p("project_id")?.trim().parse().ok()?,
                title: p(PARAM_TITLE).or_else(|| p(PARAM_TITLE_ALIAS))?.to_string(),
                description: p("description").map(str::to_string),
            }),
            XML_CARD_ROUTINE => Some(Self::CardSchedule {
                card_id: p("card_id")?.trim().parse().ok()?,
                cron: p("cron")?.to_string(),
            }),
            XML_CARD_ROUTINE_CLEAR => Some(Self::CardRoutineClear {
                card_id: p("card_id")?.trim().parse().ok()?,
            }),
            XML_CARD_RUN => Some(Self::CardRun {
                card_id: p("card_id")?.trim().parse().ok()?,
            }),
            XML_CARD_AGENT => Some(Self::CardAgent {
                card_id: p("card_id")?.trim().parse().ok()?,
                agent: p("agent")?.trim().to_string(),
            }),
            XML_CARD_IMAGE => Some(Self::CardImage {
                card_id: p("card_id")?.trim().parse().ok()?,
                image: p("image").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            }),
            XML_BOARD_LIST => Some(Self::BoardList),
            XML_CARD_FIND | XML_CARD_FIND_ALIAS => {
                let query = p("query")?.trim().to_string();
                if query.is_empty() {
                    return None;
                }
                Some(Self::CardFind { query })
            }
            XML_MATH | XML_MATH_ALIAS => {
                let expr = p("expr")?.trim().to_string();
                if expr.is_empty() {
                    return None;
                }
                Some(Self::Math(expr))
            }
            XML_LSP => Some(Self::Lsp {
                op: LspOp::parse(p("op")?)?,
                path: p("path")?.trim().to_string(),
                line: p("line")?.trim().parse().ok()?,
                col: p("col")?.trim().parse().ok()?,
            }),
            XML_AGENT_RUN => Self::parse_agent_run(&format!(
                "{} {}",
                p("agent")?.trim(),
                p("command")?.trim()
            )),
            XML_GIT => {
                let op = p("op").map(str::trim);
                let url = p("url").map(|u| u.trim().to_string());
                let agent = p("agent")
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty());
                let call = match op {
                    Some(o) if o.eq_ignore_ascii_case(GIT_OP_COMMIT) => {
                        Self::parse_git(&format!("{GIT_OP_COMMIT} {}", p("message")?))
                    }
                    Some(o) if o.eq_ignore_ascii_case(GIT_OP_BRANCH) => {
                        Self::parse_git(&format!("{GIT_OP_BRANCH} {}", p("name")?))
                    }
                    Some(o) if o.eq_ignore_ascii_case(GIT_OP_PUSH) => {
                        Self::parse_git(&format!("{GIT_OP_PUSH} {}", p("branch")?.trim()))
                    }
                    Some(o) if o.eq_ignore_ascii_case(GIT_OP_PR) => Self::parse_git(&format!(
                        "{GIT_OP_PR} {} {PR_ARG_SEP} {}",
                        p(PARAM_TITLE)?,
                        p("base").unwrap_or("")
                    )),
                    _ => Self::parse_git_op(op, url.as_deref()),
                }?;
                match call {
                    Self::Git { op, .. } => Some(Self::Git { op, agent }),
                    other => Some(other),
                }
            }
            _ => None,
        }
    }

    /// `AGENT_RUN <agent> <cmd...>` — the command may contain spaces.
    fn parse_agent_run(arg: &str) -> Option<Self> {
        let (agent, cmd) = arg.split_once(' ')?;
        let agent = agent.trim();
        let cmd = cmd.trim();
        if agent.is_empty() || cmd.is_empty() {
            return None;
        }
        Some(Self::AgentRun {
            agent: agent.to_string(),
            cmd: cmd.to_string(),
        })
    }

    /// `LSP <op> <path> <line> <col>` — the path must not contain spaces.
    fn parse_lsp(arg: &str) -> Option<Self> {
        let parts: Vec<&str> = arg.split_whitespace().collect();
        if parts.len() != LSP_ARGS {
            return None;
        }
        Some(Self::Lsp {
            op: LspOp::parse(parts[0])?,
            path: parts[1].to_string(),
            line: parts[2].parse().ok()?,
            col: parts[3].parse().ok()?,
        })
    }

    /// Splits a trailing `@name` token off a sandbox git op's args.
    fn split_agent(arg: &str) -> (&str, Option<String>) {
        match arg.rsplit_once(' ') {
            Some((rest, token)) if token.starts_with(AGENT_PREFIX) && token.len() > 1 => (
                rest.trim_end(),
                Some(token[AGENT_PREFIX.len()..].to_string()),
            ),
            _ => (arg, None),
        }
    }

    fn parse_git(arg: &str) -> Option<Self> {
        let (op, tail) = match arg.split_once(' ') {
            Some((op, tail)) => (op, Some(tail.trim())),
            None => (arg, None),
        };
        let op_upper = op.to_uppercase();
        match op_upper.as_str() {
            GIT_OP_BRANCH => {
                let (name, agent) = Self::split_agent(tail.filter(|s| !s.is_empty())?);
                if name.is_empty() {
                    return None;
                }
                Some(Self::Git {
                    op: GitOp::Branch {
                        name: name.to_string(),
                    },
                    agent,
                })
            }
            GIT_OP_COMMIT => {
                let (message, agent) = Self::split_agent(tail.filter(|s| !s.is_empty())?);
                if message.is_empty() {
                    return None;
                }
                Some(Self::Git {
                    op: GitOp::Commit {
                        message: message.to_string(),
                    },
                    agent,
                })
            }
            GIT_OP_PUSH => {
                let (branch, agent) = Self::split_agent(tail.unwrap_or(""));
                Some(Self::Git {
                    op: GitOp::Push {
                        branch: branch.to_string(),
                        url: None,
                        token: None,
                    },
                    agent,
                })
            }
            GIT_OP_PR => {
                let arg = tail?;
                let (arg, agent) = Self::split_agent(arg);
                let (title, base) = match arg.split_once(PR_ARG_SEP) {
                    Some((title, base)) => (title.trim(), base.trim()),
                    None => (arg, ""),
                };
                if title.is_empty() {
                    return None;
                }
                Some(Self::Git {
                    op: GitOp::PullRequest {
                        title: title.to_string(),
                        head: String::new(),
                        base: base.to_string(),
                        url: None,
                        token: None,
                    },
                    agent,
                })
            }
            _ => Self::parse_git_op(Some(op), tail),
        }
    }

    fn parse_git_op(op: Option<&str>, url: Option<&str>) -> Option<Self> {
        match op?.to_uppercase().as_str() {
            GIT_OP_CLONE => Some(Self::Git {
                op: GitOp::Clone {
                    url: url.filter(|u| !u.is_empty()).map(str::to_owned),
                    token: None,
                },
                agent: None,
            }),
            GIT_OP_STATUS if url.is_none() => Some(Self::Git {
                op: GitOp::Status,
                agent: None,
            }),
            GIT_OP_DIFF if url.is_none() => Some(Self::Git {
                op: GitOp::Diff,
                agent: None,
            }),
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
