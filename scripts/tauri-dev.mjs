// `npm run tauri:dev`: `tauri dev` on a port that can be bound.
//
// Keeps 1420 (the port in tauri.conf.json) when it is free. Otherwise it picks the next free one,
// tells Vite through AETHER_DEV_PORT (see vite.config.ts) and tells Tauri through a one-off config
// override, so the window loads the same address the dev server listens on.
// `--dry-run` only prints the port. Other arguments go to `tauri dev`.

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import process from "node:process";
import { defaultPort, findPort } from "./dev-port.mjs";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const rest = args.filter((arg) => arg !== "--dry-run");

const wanted = Number(process.env.AETHER_DEV_PORT ?? defaultPort);
if (!Number.isInteger(wanted) || wanted < 1024 || wanted > 65000) {
  console.error(`AETHER_DEV_PORT must be a port from 1024 to 65000, not "${process.env.AETHER_DEV_PORT}"`);
  process.exit(2);
}

const port = await findPort(wanted);
if (port !== wanted) {
  console.log(`port ${wanted} cannot be used here (a system-reserved range?); using ${port}`);
}
if (dryRun) {
  console.log(port);
  process.exit(0);
}

const require = createRequire(import.meta.url);
const tauri = require.resolve("@tauri-apps/cli/tauri.js");
const override = JSON.stringify({ build: { devUrl: `http://localhost:${port}` } });

const child = spawn(process.execPath, [tauri, "dev", "--config", override, ...rest], {
  stdio: "inherit",
  env: { ...process.env, AETHER_DEV_PORT: String(port) },
});
child.on("exit", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
