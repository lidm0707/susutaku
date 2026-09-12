import { createE2EDatabase, databaseUrl } from "./global-setup";

export default async function globalSetup() {
  const skipped = Boolean(process.env.E2E_SKIP_DB_LIFECYCLE);
  await createE2EDatabase();
  if (!skipped) {
    process.stdout.write(`[e2e] created database (internal): ${databaseUrl()}\n`);
  }
}
