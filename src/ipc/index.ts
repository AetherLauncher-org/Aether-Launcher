import type { JavaRuntimeInfo } from "./java";
import { invoke } from "@tauri-apps/api/core";

/**
 * 前端能调用的全部后端命令都登记在这里。
 * 新增命令时先在 src-tauri 注册，再把名字和返回类型补到 CommandResults。
 *
 * 具体命令的封装按领域拆在同目录的其他文件里（`call`、`instances`、
 * `java`、`accounts`），这里保留应用级信息与通用工具，并把它们统一
 * re-export 出去，组件始终从 `./ipc` 导入，不直接引用子模块。
 */
export type ApplicationInfo = {
  name: string;
  version: string;
  coreStatus: string;
};

type CommandResults = {
  application_info: ApplicationInfo;
  list_java_runtimes: JavaRuntimeInfo[];
};

export const defaultApplicationInfo: ApplicationInfo = {
  name: "Aether Launcher",
  version: "0.1.0",
  coreStatus: "foundation",
};

/** 在浏览器里单独跑前端（npm run dev）时没有 Tauri 运行时。 */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function call<Name extends keyof CommandResults>(
  name: Name,
): Promise<CommandResults[Name]> {
  return invoke<CommandResults[Name]>(name);
}

export function getApplicationInfo(): Promise<ApplicationInfo> {
  return call("application_info");
}

/** Preserve the Java discovery API used by PR #12. */
export function listJavaRuntimes(): Promise<JavaRuntimeInfo[]> {
  return call("list_java_runtimes");
}

// 带参数的命令封装与错误描述
export { callCommand, describeCommandError } from "./call";

// 实例列表（实例由安装流程创建）
export { emptyInstanceListing, listInstances } from "./instances";
export type {
  InstanceListing,
  InstanceState,
  InstanceSummary,
  SkippedInstance,
} from "./instances";

// 官方 Minecraft 版本目录（只读）
export { listMinecraftVersions } from "./catalog";
export type { MinecraftCatalogVersion, MinecraftVersionCatalog } from "./catalog";

// 一键安装：命令与进度事件
export {
  cancelVanillaInstall,
  installProgressEvent,
  isInstallCancelled,
  listenInstallProgress,
  startVanillaInstall,
} from "./install";
export type { InstallProgress, InstallStage } from "./install";

// Java 运行时发现
export { discoverJavaRuntimes } from "./java";
export type { JavaRuntimeInfo } from "./java";

// 微软账户登录
export { describeLoginStatus, startMicrosoftLogin } from "./accounts";
export type { MicrosoftLoginStatus } from "./accounts";

// Installed game lifecycle (offline only; no arbitrary executable or paths)
export { gameStatus, startInstalledInstance, stopInstalledInstance } from "./launch";
export type { GameRunStatus } from "./launch";

// 无边框窗口的控制
export {
  closeWindow,
  minimizeWindow,
  setWindowTheme,
  toggleMaximizeWindow,
  watchMaximized,
} from "./window";
