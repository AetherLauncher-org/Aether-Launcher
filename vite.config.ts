import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;
// 1420 是 tauri.conf.json 里的端口。Windows 会保留一些端口段（重启后 Hyper-V / WinNAT 随机占），
// 落在里面的端口绑不上（EACCES）。`npm run tauri:dev` 会挑一个能用的并通过这个变量告诉 Vite。
const devPort = Number(process.env.AETHER_DEV_PORT) || 1420;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    // 每个 jsdom + Fluent 的 worker 都要占几百 MB；默认按核数并行，内存紧的机器上
    // 会因为 V8 堆耗尽而整个 worker 异常退出（exit code 134）。限制并行数换稳定。
    maxWorkers: 2,
    css: true,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: devPort,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: devPort + 1,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
