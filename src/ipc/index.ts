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

// 从现有 .minecraft 或其他启动器导入实例
export {
  detectLauncherInstallations,
  importVersions,
  listenImportItem,
  pickImportFolder,
  pickLauncherFolder,
} from "./import";
export type {
  ImportCandidate,
  ImportItemProgress,
  ImportResult,
  ImportSkipped,
  ImportSource,
  ImportSourceKind,
  LauncherInstallation,
} from "./import";

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

// 本机内存（只读）
export { getSystemMemory } from "./system";
export type { SystemMemory } from "./system";

// Java 运行时发现
export { discoverJavaRuntimes, pickJavaRuntime } from "./java";
export type { JavaRuntimeInfo } from "./java";

// 微软账户登录
export {
  beginMicrosoftBrowserLogin,
  beginMicrosoftLogin,
  cancelMicrosoftLogin,
  describeLoginDetail,
  describeLoginFailure,
  describeMicrosoftError,
  getAccountSkin,
  isMicrosoftLoginAvailable,
  listenLoginStatus,
  listMicrosoftAccounts,
  openMicrosoftVerificationPage,
  removeMicrosoftAccount,
} from "./accounts";
export type {
  BrowserLoginInfo,
  DeviceCodeInfo,
  LoginEvent,
  LoginFailure,
  LoginFailureReason,
  MicrosoftAccount,
} from "./accounts";

// Installed game lifecycle (no arbitrary executable or paths)
export { gameStatus, startInstalledInstance, stopInstalledInstance } from "./launch";
export type { GameRunStatus } from "./launch";

// 投影原理图：3D 预览、转换
export { convertInstanceSchematic, getConversionTargets, readInstanceSchematic } from "./schematics";
export type {
  BlockEntityView,
  ConversionResult,
  ConversionTargets,
  ItemView,
  PaletteEntry,
  SchematicFormat,
  SchematicView,
} from "./schematics";

// 内容下载：搜索 Modrinth、装进实例或世界
export { contentTypes, getContentTarget, installContent, searchContent } from "./content";
export type {
  ContentTarget,
  ContentType,
  InstallReport,
  InstalledFile,
  SearchHit,
  SearchResult,
  SearchSort,
  SkippedProject,
} from "./content";

// 实例管理：详情、改名、删除、内容文件夹、日志、打开文件夹
export {
  contentKinds,
  deleteInstance,
  getInstanceDetails,
  getWorldDetails,
  identifyContent,
  isLookupKind,
  listInstanceContent,
  listInstanceLogs,
  openContentProject,
  openInstanceFolder,
  openWorldFolder,
  readInstanceLog,
  removeContent,
  removeWorldDatapack,
  renameInstance,
  setModEnabled,
} from "./manage";
export type {
  ContentCounts,
  ContentItem,
  ContentKind,
  ContentLookup,
  DatapackInfo,
  FolderTarget,
  IdentifiedContent,
  InstanceDetails,
  LogContent,
  LogEntry,
  LookupKind,
  PackState,
  WorldDetails,
} from "./manage";

// 无边框窗口的控制
export {
  closeWindow,
  minimizeWindow,
  setWindowTheme,
  toggleMaximizeWindow,
  watchMaximized,
} from "./window";

// 模组加载器：可用版本列表与安装
export {
  installModLoaderVersion,
  listModLoaderVersions,
} from "./modloader";
export type {
  ModLoaderChoice,
  ModLoaderInstallResult,
  ModLoaderKind,
} from "./modloader";
