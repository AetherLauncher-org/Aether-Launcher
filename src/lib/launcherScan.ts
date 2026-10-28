import type { ImportSourceKind, LauncherInstallation } from "../ipc";

/**
 * 「扫描本机」的结果摘要。
 *
 * 纯粹想给出「找到几个启动器、共几个版本」，不让首页去关心每个启动器的
 * 细节 —— 那些留给导入对话框。抽成纯函数便于单测。
 */
export type LauncherScan = {
  installations: LauncherInstallation[];
  /** 所有安装里出现过的版本名去重后的数量。 */
  totalVersions: number;
};

/** 把一次探测结果压成首页要显示的摘要。 */
export function summariseScan(installations: LauncherInstallation[]): LauncherScan {
  const versions = new Set<string>();
  for (const installation of installations) {
    for (const source of installation.sources) {
      for (const candidate of source.candidates) {
        versions.add(candidate.displayName);
      }
    }
  }
  return { installations, totalVersions: versions.size };
}

/** 启动器的展示名，用于摘要文案。 */
export function launcherLabel(kind: ImportSourceKind): string {
  switch (kind) {
    case "pcl2":
      return "PCL2";
    case "hmcl":
      return "HMCL";
    case "bakaxl":
      return "BakaXL";
    case "prism":
      return "Prism / MultiMC";
    case "folder":
      return "游戏目录";
  }
}
