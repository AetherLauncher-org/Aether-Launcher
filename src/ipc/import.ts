import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { callCommand } from "./call";
import { isTauri } from "./index";
import type { InstanceSummary } from "./instances";

/**
 * 从现有启动器导入实例的类型化封装。
 *
 * 命令定义在 `src-tauri/src/commands/import.rs`：
 * - `pick_import_folder` 在**后端**打开系统文件夹选择框，扫描用户选中的目录并
 *   返回预览；取消选择时返回 `null`。
 * - `detect_launcher_installations` 在有限范围内查找 PCL2、HMCL、BakaXL、
 *   Prism Launcher / MultiMC，并读取各自配置里记录的游戏目录或实例。
 * - `pick_launcher_folder` 让用户手动指定启动器目录或 `.minecraft` 目录，用于
 *   自动查找失败的情况。
 * - `import_versions` 只接收预览返回的 `token` 和用户勾选的条目 ID。每个版本都按
 *   官方清单安装到启动器自己的游戏目录，来源文件夹只作为只读的"种子"：哈希一致的
 *   文件直接复制，其余照常下载。进度走 `install://progress` 和 `import://item`
 *   事件，可用 `cancelVanillaInstall` 取消。
 *
 * 前端从不传路径：`token` 是后端保存的扫描结果句柄，且只能用一次。
 */

declare module "./call" {
  interface CommandArgs {
    pick_import_folder: void;
    detect_launcher_installations: void;
    pick_launcher_folder: void;
    import_versions: { token: string; ids: string[] };
  }

  interface CommandResults {
    pick_import_folder: ImportSource | null;
    detect_launcher_installations: LauncherInstallation[];
    pick_launcher_folder: LauncherInstallation | null;
    import_versions: ImportResult;
  }
}

/** 与 `aether_storage::ImportSourceKind` 的稳定标签一一对应。 */
export type ImportSourceKind = "pcl2" | "hmcl" | "bakaxl" | "prism" | "folder";

/**
 * 一个可导入的条目。
 *
 * 游戏目录来源里是版本号，Prism / MultiMC 来源里是实例文件夹名；两者都是后端
 * 会再次校验的单段路径名。
 */
export type ImportCandidate = {
  id: string;
  displayName: string;
};

/** 一次扫描的结果；`token` 是后端句柄，不是路径。 */
export type ImportSource = {
  token: string;
  sourceKind: ImportSourceKind;
  sourcePath: string;
  candidates: ImportCandidate[];
};

/**
 * 一个检测到的启动器，以及它能贡献的来源。
 *
 * PCL2 / HMCL / BakaXL 每个游戏目录算一个来源；Prism / MultiMC 只有一个来源，
 * 候选就是它的全部实例。
 */
export type LauncherInstallation = {
  kind: ImportSourceKind;
  launcherRoot: string;
  sources: ImportSource[];
};

/** 导入时被跳过的条目，只带稳定分类。 */
export type ImportSkipped = {
  id: string;
  reason: string;
};

export type ImportResult = {
  imported: InstanceSummary[];
  skipped: ImportSkipped[];
};

/** 每个版本开始安装前推送的事件。 */
export const importItemEvent = "import://item";

export type ImportItemProgress = {
  id: string;
  index: number;
  total: number;
};

const desktopOnly = "需要桌面版才能从现有启动器导入";

/** 打开系统文件夹选择框并扫描选中的目录；取消时返回 `null`。 */
export function pickImportFolder(): Promise<ImportSource | null> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("pick_import_folder");
}

/** 自动查找已安装的启动器；找不到时返回空数组，不是错误。 */
export function detectLauncherInstallations(): Promise<LauncherInstallation[]> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("detect_launcher_installations");
}

/** 手动指定启动器目录或游戏目录；取消时返回 `null`。 */
export function pickLauncherFolder(): Promise<LauncherInstallation | null> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("pick_launcher_folder");
}

/** 把预览里勾选的条目导入为受管实例。 */
export function importVersions(
  token: string,
  ids: readonly string[],
): Promise<ImportResult> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("import_versions", {
    token,
    ids: [...ids],
  });
}

/** 订阅导入进度。浏览器预览里没有事件源，返回一个空的取消函数。 */
export async function listenImportItem(
  handler: (item: ImportItemProgress) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => undefined;
  }
  return listen<ImportItemProgress>(importItemEvent, (event) =>
    handler(event.payload),
  );
}
