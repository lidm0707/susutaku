import { dropE2EDatabase } from "./global-setup";

export default async function globalTeardown() {
  await dropE2EDatabase();
  process.stdout.write("[e2e] dropped database\n");
}
