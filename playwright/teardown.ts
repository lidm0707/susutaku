import { dropE2EDatabase } from "./global-setup";

export default async function globalTeardown() {
  await dropE2EDatabase();
  if (!process.env.E2E_SKIP_DB_LIFECYCLE) {
    process.stdout.write("[e2e] dropped database\n");
  }
}
