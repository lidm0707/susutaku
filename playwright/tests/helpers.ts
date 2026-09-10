import { request as pwRequest } from "@playwright/test";

export const API = process.env.PLAYWRIGHT_BASE_URL || "http://localhost:3334";
const ADMIN_USER = process.env.E2E_ADMIN_USER || "owner";
let ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD || "owner";
export const E2E_USER = process.env.E2E_USER || "e2e-tester";
export const E2E_PASSWORD = process.env.E2E_PASSWORD || "e2e-e2e-e2e";

export async function seedUser() {
  const ctx = await pwRequest.newContext({ baseURL: API });
  // Zero users -> unauthenticated bootstrap creates the owner.
  let res = await ctx.post("/api/auth/users", {
    data: { username: E2E_USER, password: E2E_PASSWORD, role: "owner" },
  });
  if (res.ok()) {
    await ctx.dispose();
    return;
  }
  // Otherwise an admin logs in and creates the e2e user. A previous run may
  // have rotated the admin password, so retry with the rotated one on 401.
  const ROTATED_PASSWORD = process.env.E2E_ADMIN_NEW_PASSWORD || "owner-owner-1";
  let login = await ctx.post("/api/auth/login", {
    data: { username: ADMIN_USER, password: ADMIN_PASSWORD },
  });
  if (!login.ok() && ADMIN_PASSWORD !== ROTATED_PASSWORD) {
    login = await ctx.post("/api/auth/login", {
      data: { username: ADMIN_USER, password: ROTATED_PASSWORD },
    });
    if (login.ok()) ADMIN_PASSWORD = ROTATED_PASSWORD;
  }
  if (!login.ok()) {
    throw new Error(
      `cannot seed e2e user: bootstrap=${res.status()} admin login=${login.status()}. `
        + `Set E2E_ADMIN_USER / E2E_ADMIN_PASSWORD in docker/docker-compose.playwright.yml.`
    );
  }
  const body = await login.json();
  const token: string = body.token;
  if (body.must_change_password) {
    // Default-seeded owner must set a new password before it can be used.
    const newAdminPassword = process.env.E2E_ADMIN_NEW_PASSWORD || "owner-owner-1";
    await ctx.post("/api/auth/change-password", {
      headers: { Authorization: `Bearer ${token}` },
      data: { old_password: ADMIN_PASSWORD, new_password: newAdminPassword },
    });
    ADMIN_PASSWORD = newAdminPassword;
  }
  res = await ctx.post("/api/auth/users", {
    headers: { Authorization: `Bearer ${token}` },
    data: { username: E2E_USER, password: E2E_PASSWORD, role: "editor" },
  });
  const createStatus = res.status();
  const createBody = await res.text();
  await ctx.dispose();
  const userReady = res.ok() || createStatus === 409 || createBody.includes("already taken");
  if (!userReady) {
    throw new Error(`create e2e user failed: ${createStatus} ${createBody}`);
  }
}

export function e2eCredentials() {
  return { username: E2E_USER, password: E2E_PASSWORD };
}

export async function loginToken(
  username: string = E2E_USER,
  password: string = E2E_PASSWORD,
): Promise<string> {
  const ctx = await pwRequest.newContext({ baseURL: API });
  const res = await ctx.post("/api/auth/login", {
    data: { username, password },
  });
  const body = await res.text();
  await ctx.dispose();
  if (!res.ok()) {
    throw new Error(`login ${username} failed: ${res.status()} ${body}`);
  }
  return (JSON.parse(body) as { token: string }).token;
}
