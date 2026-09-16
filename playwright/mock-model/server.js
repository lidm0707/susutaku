// Mock of the local MLX model server (proto-rs) for e2e runs: the backend's
// RemoteModel talks to /api/models, /api/models/select and /api/inference.
// Replies are deterministic so tests never need a real model:
// - prompt offers tools and has no results yet -> one TOOL: line (the backend
//   runs it and comes back with "Tool results:")
// - prompt carries tool results -> final answer with the tool output marker
// - prompt asks to summarize -> fixed MOCK-SUMMARY line (the /compact case)
// - otherwise -> echo of the user message (it is the last line of the prompt)
const http = require("node:http");

const PORT = Number(process.env.MOCK_MODEL_PORT || 8992);
const MODEL_NAME = "e2e-mock";
const MODEL_BYTES = 4_800_000_000;
const MODEL_ENGINE = "mock";

const TOOL_OFFER_MARKER = "You HAVE web tools";
const TOOL_RESULT_MARKER = "Tool results:";
const SUMMARY_TRIGGER = "summarize";

const TOOL_REPLY = "TOOL: SHELL echo toolcall-ok";
const TOOL_DONE_REPLY = "TOOLCALL-OK: shell tool ran and returned toolcall-ok.";
const SUMMARY_REPLY = "MOCK-SUMMARY: e2e context compacted by the mock model.";
const ECHO_PREFIX = "echo:";

// Scripted "do task:" flow (card+agent flow, pipelines removed): the mock
// walks the same tool sequence a real model would — BOARD_LIST, CARD_FIND,
// CARD_CREATE (or reuse), CARD_AGENT (ALWAYS assign an agent), CARD_ROUTINE —
// driven by the tool results accumulated in the prompt. Steps are keyed on
// the number of "Tool results:" headers (one per round), so no assumption
// about individual tool result wording is needed beyond find-or-create.
const TASK_TRIGGER = "do task:";
const TASK_AGENT_PARAM = "default";
const TASK_CRON = "0 * * * *";
const TASK_OK_PREFIX = "TASK-OK:";

function lastMatch(re, s) {
  const m = [...s.matchAll(new RegExp(re, "g"))].pop();
  return m || null;
}

function taskFlow(prompt) {
  const topic = new RegExp(`${TASK_TRIGGER}\\s*(.+)`, "i").exec(prompt)[1].trim();
  const title = topic.split(/\s+/).slice(0, 6).join(" ");
  const esc = title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Every tool round appends its own "Tool results:" header.
  const rounds = (prompt.match(/Tool results:/g) || []).length;
  if (rounds === 0) return "TOOL: BOARD_LIST";
  const markerIdx = prompt.indexOf("Tool results:");
  const results = prompt.slice(markerIdx);
  if (rounds === 1) return `TOOL: CARD_FIND ${title}`;

  const created = lastMatch("card (\\d+) created in project", results);
  const found = lastMatch("card (\\d+) `" + esc + "`", results);
  const cardId = created ? created[1] : found ? found[1] : null;
  if (!cardId) {
    const pid = lastMatch("project (\\d+) `", results);
    return `TOOL: CARD_CREATE ${pid[1]} ${title} | plan: ${topic}`;
  }

  // find-or-create done -> ALWAYS assign an agent, then set the routine.
  if (rounds === 2) return `TOOL: CARD_AGENT ${cardId} ${TASK_AGENT_PARAM}`;
  if (rounds === 3) return `TOOL: CARD_ROUTINE ${cardId} ${TASK_CRON}`;
  return `${TASK_OK_PREFIX} task stored on card ${cardId}: ${title}`;
}

const STATS = {
  prompt_tokens: 8,
  prompt_secs: 0.01,
  decode_tokens: 4,
  decode_secs: 0.02,
};

const HTTP_OK = 200;

function replyFor(prompt) {
  if (prompt.includes(TOOL_OFFER_MARKER) && !prompt.includes(TOOL_RESULT_MARKER)) {
    return prompt.toLowerCase().includes(TASK_TRIGGER)
      ? "TOOL: BOARD_LIST"
      : TOOL_REPLY;
  }
  if (prompt.includes(TOOL_RESULT_MARKER)) {
    if (prompt.toLowerCase().includes(TASK_TRIGGER)) return taskFlow(prompt);
    return TOOL_DONE_REPLY;
  }
  if (prompt.toLowerCase().includes(SUMMARY_TRIGGER)) {
    return SUMMARY_REPLY;
  }
  // Contexted (tools-off) prompts echo the [context: …] line, so e2e can
  // assert the send path. The context line is located anywhere in the prompt:
  // instructions (plot fence etc.) precede it.
  if (prompt.includes("[context:")) {
    const line =
      prompt.split("\n").find((l) => l.trim().startsWith("[context:")) || "";
    return `${ECHO_PREFIX} ${line.trim()}`;
  }
  const lastLine = prompt.trim().split("\n").pop() || "";
  return `${ECHO_PREFIX} ${lastLine}`;
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => resolve(raw));
    req.on("error", reject);
  });
}

function sendJson(res, body) {
  const payload = JSON.stringify(body);
  res.writeHead(HTTP_OK, { "content-type": "application/json" });
  res.end(payload);
}

const server = http.createServer(async (req, res) => {
  if (req.method === "GET" && req.url === "/api/models") {
    sendJson(res, [
      {
        name: MODEL_NAME,
        loadable: true,
        bytes: MODEL_BYTES,
        engine: MODEL_ENGINE,
        selected: true,
      },
    ]);
    return;
  }
  if (req.method === "POST" && req.url === "/api/models/select") {
    const body = JSON.parse((await readBody(req)) || "{}");
    sendJson(res, { selected: body.name || MODEL_NAME });
    return;
  }
  if (req.method === "POST" && req.url === "/api/inference") {
    const body = JSON.parse((await readBody(req)) || "{}");
    sendJson(res, {
      model: MODEL_NAME,
      text: replyFor(String(body.prompt || "")),
      stats: STATS,
    });
    return;
  }
  res.writeHead(404, { "content-type": "text/plain" });
  res.end("not found");
});

server.listen(PORT, () => {
  process.stdout.write(`mock model server on :${PORT}\n`);
});
