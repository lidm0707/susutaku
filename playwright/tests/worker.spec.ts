import { request as pwRequest } from "@playwright/test";
import { API, E2E_PASSWORD, E2E_USER, loginToken } from "./helpers";
import { expect, test } from "./fixtures";

const VIEWER_USER = "e2e-viewer";
const VIEWER_PASSWORD = "viewer-viewer-viewer";
const EVERY_MINUTE = "* * * * *";
const WORKER_TIMEOUT_MS = 120_000;
const WORKER_POLL_MS = 2_000;
const CRON_TICK_MS = 40_000;

type RunRecord = {
  status: string;
  output?: string | null;
  stages: { node: string; stage: string; status: string; note: string }[];
};

type CronJob = { card_id: number; title: string; cron: string; next_run: number };

async function viewerToken(): Promise<string | null> {
  const ctx = await pwRequest.newContext({ baseURL: API });
  const res = await ctx.post("/api/auth/users", {
    headers: { Authorization: `Bearer ${await loginToken()}` },
    data: { username: VIEWER_USER, password: VIEWER_PASSWORD, role: "viewer" },
  });
  await ctx.dispose();
  if (!res.ok()) return null;
  return loginToken(VIEWER_USER, VIEWER_PASSWORD);
}

test.describe("schedule worker", () => {
  test("auth guards: cronjobs and scheduling need a token, viewer is read-only", async ({
    request,
  }) => {
    let res = await request.get("/api/cronjobs");
    expect(res.status()).toBe(401);

    res = await request.put("/api/kanban/cards/1/schedule", {
      data: { cron: EVERY_MINUTE },
    });
    expect(res.status()).toBe(401);

    const viewer = await viewerToken();
    test.skip(viewer === null, "e2e user cannot manage users on this database");
    const vHeaders = { Authorization: `Bearer ${viewer}` };

    res = await request.put("/api/kanban/cards/1/schedule", {
      headers: vHeaders,
      data: { cron: EVERY_MINUTE },
    });
    expect(res.status()).toBe(403);

    res = await request.get("/api/cronjobs", { headers: vHeaders });
    expect(res.ok()).toBeTruthy();
  });

  test("backend worker picks up a scheduled card and runs its pipeline", async ({
    request,
  }) => {
    test.setTimeout(WORKER_TIMEOUT_MS + 60_000);
    const headers = { Authorization: `Bearer ${await loginToken()}` };

    let res = await request.post("/api/workspaces", {
      headers,
      data: { name: `e2e-worker-${Date.now()}` },
    });
    expect(res.ok()).toBeTruthy();
    const ws = await res.json();

    res = await request.post(`/api/workspaces/${ws.id}/projects`, {
      headers,
      data: { name: "worker-project" },
    });
    expect(res.ok()).toBeTruthy();
    const project = await res.json();

    const spec = {
      nodes: [
        { id: "seed", stage: "ingest", params: {} },
        { id: "shout", stage: "transform", params: { op: "upper" } },
      ],
      links: [{ from: "seed", to: "shout" }],
    };
    res = await request.post("/api/pipelines", {
      headers,
      data: { name: `e2e-worker-pipeline-${Date.now()}`, spec },
    });
    expect(res.ok()).toBeTruthy();
    const pipeline = await res.json();

    res = await request.post("/api/kanban/cards", {
      headers,
      data: {
        project_id: project.id,
        column_id: "todo",
        title: "worker e2e card",
        description: "scheduled pipeline run",
      },
    });
    expect(res.ok()).toBeTruthy();
    const card = await res.json();

    res = await request.put(`/api/kanban/cards/${card.id}/pipeline`, {
      headers,
      data: { pipeline_id: pipeline.id },
    });
    expect(res.ok()).toBeTruthy();

    res = await request.put(`/api/kanban/cards/${card.id}/schedule`, {
      headers,
      data: { cron: EVERY_MINUTE },
    });
    expect(res.ok()).toBeTruthy();

    // The backend's own scheduler worker ticks and registers the next run.
    let listed: CronJob | undefined;
    await expect
      .poll(
        async () => {
          const jobsRes = await request.get("/api/cronjobs", { headers });
          const jobs = jobsRes.ok() ? ((await jobsRes.json()) as CronJob[]) : [];
          listed = jobs.find((j) => j.card_id === card.id);
          return listed?.next_run ?? 0;
        },
        { timeout: WORKER_TIMEOUT_MS, intervals: [WORKER_POLL_MS] },
      )
      .toBeGreaterThan(0);
    expect(listed?.cron).toBe(EVERY_MINUTE);

    // The worker runs the due pipeline and records it on the card's agent state.
    let record: RunRecord | undefined;
    await expect
      .poll(
        async () => {
          const agentRes = await request.get(`/api/kanban/cards/${card.id}/agent`, {
            headers,
          });
          if (!agentRes.ok()) return "";
          const agent = (await agentRes.json()) as { state?: { run?: RunRecord } };
          record = agent.state?.run;
          return record?.status ?? "";
        },
        { timeout: WORKER_TIMEOUT_MS, intervals: [WORKER_POLL_MS] },
      )
      .toBe("ok");
    expect(record?.output).toBe("WORKER E2E CARD\n\nSCHEDULED PIPELINE RUN");
    expect(record?.stages.map((s) => s.stage)).toEqual(["ingest", "transform"]);

    // Unschedule: the worker drops the card from its cron registry.
    res = await request.put(`/api/kanban/cards/${card.id}/schedule`, {
      headers,
      data: { cron: null },
    });
    expect(res.ok()).toBeTruthy();
    await expect
      .poll(
        async () => {
          const jobsRes = await request.get("/api/cronjobs", { headers });
          const jobs = jobsRes.ok() ? ((await jobsRes.json()) as CronJob[]) : [];
          return jobs.some((j) => j.card_id === card.id);
        },
        { timeout: WORKER_TIMEOUT_MS, intervals: [CRON_TICK_MS] },
      )
      .toBe(false);

    await request.delete(`/api/workspaces/${ws.id}`, { headers });
  });
});
