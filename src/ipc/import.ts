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
 * - `import_versions` 只接收预览返回的 `token` 和用户勾选的条目 ID。
 *
 * 前端从不传路径：`token` 是后端保存的扫描结果句柄，且只能用一次。
 */

declare module "./call" {
  interface CommandArgs {
    pick_import_folder: void;
    detect_launcher_installations: void;
    pick_launcher_folder: void;
    import_versions: { token: string; ids: string[] };
    install_imported_instance: { id: string; needsManifest: boolean };
  }

  interface CommandResults {
    pick_import_folder: ImportSource | null;
    detect_launcher_installations: LauncherInstallation[];
    pick_launcher_folder: LauncherInstallation | null;
    import_versions: ImportResult;
    install_imported_instance: number;
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

/**
 * 给刚导入的实例补齐客户端 jar、libraries、natives 和 assets。
 * 导入只复制版本元数据，这一步才真正下载游戏文件。目标目录由后端按实例的
 * `game_directory` 决定，前端只传实例 ID。`needsManifest` 用于 Prism / MultiMC
 * 来源：它们没有可复制的版本 JSON，需要先从官方目录取回并校验。
 *
 * 下载期间和正式安装一样发 `install://progress` 事件，并且可以被
 * `cancelVanillaInstall` 取消；已校验通过的文件不重复下载，所以重复调用可以续传。
 */
export function installImportedInstance(
  id: string,
  needsManifest: boolean,
): Promise<number> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("install_imported_instance", { id, needsManifest });
}
