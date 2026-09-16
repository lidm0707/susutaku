import { request as pwRequest, type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

// Minimal card-run coverage (replaces the old pipeline editor spec): a card
// runs its ASSIGNED agent via POST /api/task/cards/{id}/run; a card without
// an agent gets a failed run ("no agent assigned"). API-only, so it runs
// against any authenticated stack — no mock model or sandbox needed.
const RUN_TIMEOUT_MS = 15_000;

type RunRecord = { status: string; note?: string | null };

test.describe("card run", () => {
  test("a card without an agent fails its run; the run lands in history", async ({
    request,
  }) => {
    const headers = { Authorization: `Bearer ${await loginToken()}` };

    let res = await request.post("/api/workspaces", {
      headers,
      data: { name: `e2e-cardrun-${Date.now()}` },
    });
    expect(res.ok()).toBeTruthy();
    const ws = await res.json();

    res = await request.post(`/api/workspaces/${ws.id}/projects`, {
      headers,
      data: { name: "card-run" },
    });
    expect(res.ok()).toBeTruthy();
    const project = await res.json();

    res = await request.post("/api/task/cards", {
      headers,
      data: {
        project_id: project.id,
        column_id: "todo",
        title: "card run e2e",
        description: "minimal card-run check",
      },
    });
    expect(res.ok()).toBeTruthy();
    const card = await res.json();

    // No agent assigned -> the run must fail with the no-agent note.
    res = await request.post(`/api/task/cards/${card.id}/run`, { headers });
    expect(res.ok()).toBeTruthy();
    const record = (await res.json()) as RunRecord;
    expect(record.status).toContain("fail");
    expect(record.note ?? "").toContain("no agent");

    // The failed run shows up in the card's run history.
    let runs: RunRecord[] = [];
    await expect
      .poll(
        async () => {
          const runsRes = await request.get(`/api/task/cards/${card.id}/runs`, { headers });
          runs = runsRes.ok() ? ((await runsRes.json()) as RunRecord[]) : [];
          return runs.length;
        },
        { timeout: RUN_TIMEOUT_MS },
      )
      .toBeGreaterThan(0);
    expect(runs[0].status).toContain("fail");

    await request.delete(`/api/workspaces/${ws.id}`, { headers });
  });

  test("assigning an agent to a card persists on the card's agent state", async ({
    request,
  }) => {
    const headers = { Authorization: `Bearer ${await loginToken()}` };
    const ctx: APIRequestContext = await pwRequest.newContext({ baseURL: API });

    let res = await ctx.post("/api/workspaces", {
      headers,
      data: { name: `e2e-cardagent-${Date.now()}` },
    });
    const ws = await res.json();
    res = await ctx.post(`/api/workspaces/${ws.id}/projects`, {
      headers,
      data: { name: "card-agent" },
    });
    const project = await res.json();
    res = await ctx.post("/api/task/cards", {
      headers,
      data: { project_id: project.id, column_id: "todo", title: "agent assign e2e" },
    });
    const card = await res.json();

    // CARD_AGENT <card_id> <agent_name> equivalent over HTTP.
    const agentName = `e2e-runner-${Date.now()}`;
    res = await ctx.put(`/api/task/cards/${card.id}/agent`, {
      headers,
      data: { name: agentName },
    });
    expect(res.ok()).toBeTruthy();

    res = await ctx.get(`/api/task/cards/${card.id}/agent`, { headers });
    expect(res.ok()).toBeTruthy();
    const agent = (await res.json()) as { name: string };
    expect(agent.name).toBe(agentName);

    await ctx.delete(`/api/workspaces/${ws.id}`, { headers });
    await ctx.dispose();
  });
});
