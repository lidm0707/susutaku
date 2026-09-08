import { createE2EDatabase, databaseUrl } from "./global-setup";

export default async function globalSetup() {
  await createE2EDatabase();
  // The host backend must point at the fresh database for this run:
  //   DATABASE_URL=<url> cargo run -p backend
  process.stdout.write(`[e2e] created database (internal): ${databaseUrl()}\n`);
}
