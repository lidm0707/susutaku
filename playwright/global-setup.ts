import { Client } from "pg";

const PG_HOST = process.env.E2E_PG_HOST || "postgres";
const PG_PORT = Number(process.env.E2E_PG_PORT || 5432);
const PG_USER = process.env.E2E_PG_USER || "susutaku";
const PG_PASSWORD = process.env.E2E_PG_PASSWORD || "susutaku";
const DB_NAME = process.env.E2E_DB_NAME || "susutaku_e2e";

export function databaseUrl(): string {
  return `postgres://${PG_USER}:${PG_PASSWORD}@${PG_HOST}:${PG_PORT}/${DB_NAME}`;
}

async function withAdmin(run: (client: Client) => Promise<void>): Promise<void> {
  const client = new Client({
    host: PG_HOST,
    port: PG_PORT,
    user: PG_USER,
    password: PG_PASSWORD,
    database: "postgres",
  });
  await client.connect();
  try {
    await run(client);
  } finally {
    await client.end();
  }
}

export async function createE2EDatabase(): Promise<void> {
  if (process.env.E2E_SKIP_DB_LIFECYCLE) return;
  await withAdmin(async (db) => {
    await db.query(
      `SELECT pg_terminate_backend(pid) FROM pg_stat_activity`
        + ` WHERE datname = $1 AND pid <> pg_backend_pid()`,
      [DB_NAME],
    );
    await db.query(`DROP DATABASE IF EXISTS ${DB_NAME}`);
    await db.query(`CREATE DATABASE ${DB_NAME}`);
  });
}

export async function dropE2EDatabase(): Promise<void> {
  if (process.env.E2E_SKIP_DB_LIFECYCLE) return;
  await withAdmin(async (db) => {
    await db.query(
      `SELECT pg_terminate_backend(pid) FROM pg_stat_activity`
        + ` WHERE datname = $1 AND pid <> pg_backend_pid()`,
      [DB_NAME],
    );
    await db.query(`DROP DATABASE IF EXISTS ${DB_NAME}`);
  });
}
