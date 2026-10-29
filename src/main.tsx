import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/index.css";

/**
 * 开发服务器里用普通浏览器打开时没有 Tauri 后端。先装上假后端（见 `dev/mockBackend.ts`），
 * 这样界面有数据可看、登录和安装能走完整流程。网址带 `?mock=off` 时跳过。
 * 生产构建里 `import.meta.env.DEV` 是 false，这段连同假后端都会被去掉。
 */
async function start() {
  const wantsMock = new URLSearchParams(window.location.search).get("mock") !== "off";
  if (import.meta.env.DEV && wantsMock && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void start();
