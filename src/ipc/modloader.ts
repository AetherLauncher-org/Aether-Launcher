import { callCommand } from "./call";
import { isTauri } from "./index";
import type { InstanceSummary } from "./instances";

/**
 * 模组加载器的列表与安装。
 *
 * 元数据只从官方接口读：Fabric 的 `meta.fabricmc.net`、Forge 与 NeoForge 各自的
 * 官方 maven。前端传的是「哪个加载器、哪个游戏版本、哪个加载器版本」，地址由后端
 * 拼 —— 前端没有办法让它请求任意 URL。
 */

export type ModLoaderKind = "fabric" | "forge" | "neoforge" | "quilt";

declare module "./call" {
  interface CommandArgs {
    list_modloader_versions: { gameVersion: string };
    download_modloader_installer: {
      kind: ModLoaderKind;
      gameVersion: string;
      loaderVersion: string;
    };
    install_modloader_version: {
      kind: ModLoaderKind;
      gameVersion: string;
      loaderVersion: string;
      name: string;
    };
  }

  interface CommandResults {
    list_modloader_versions: ModLoaderChoice[];
    download_modloader_installer: string[];
    install_modloader_version: ModLoaderInstallResult;
  }
}

/** 一个加载器对某个游戏版本的可用情况。 */
export type ModLoaderChoice = {
  kind: ModLoaderKind;
  label: string;
  /** 这个游戏版本能不能装。 */
  available: boolean;
  /** 不能装、或者读取失败时的原因。 */
  reason: string | null;
  /** 可选的加载器版本，最新的在前。 */
  versions: string[];
};

/**
 * 列出四个加载器对这个游戏版本的可用版本。
 *
 * 任何一个加载器失败都只影响它自己那一行（`available: false` + `reason`），
 * 所以这里把整体失败也降级成空列表，由界面提示「读取失败」。
 */
export async function listModLoaderVersions(
  gameVersion: string,
): Promise<ModLoaderChoice[]> {
  if (!isTauri()) {
    return [];
  }
  try {
    return await callCommand("list_modloader_versions", { gameVersion });
  } catch {
    return [];
  }
}

/**
 * 下载某个加载器版本的安装器。
 *
 * 返回下载到本机的文件列表（绝对路径由后端给出，只用于展示）。
 * 抛错表示这个版本没有安装器或者下载失败。
 */
export async function downloadModLoaderInstaller(
  kind: ModLoaderKind,
  gameVersion: string,
  loaderVersion: string,
): Promise<string[]> {
  return callCommand("download_modloader_installer", {
    kind,
    gameVersion,
    loaderVersion,
  });
}

/** 装好一个加载器版本之后返回的结果。 */
export type ModLoaderInstallResult = {
  /** 新建的实例。 */
  instance: InstanceSummary;
  /** 加载器的版本 id，同时是 `versions/<id>/` 的目录名。 */
  versionId: string;
};

/**
 * 装好一个模组加载器版本，并建一个指向它的实例。
 *
 * 后端会先装原版、再执行加载器自己的步骤（下载库、跑 installer、写版本文件），
 * 最后把实例的版本切到加载器。进度走 `install://progress` 事件。
 *
 * `name` 留空时用加载器的版本 id 作为实例名。
 */
export async function installModLoaderVersion(
  kind: ModLoaderKind,
  gameVersion: string,
  loaderVersion: string,
  name = "",
): Promise<ModLoaderInstallResult> {
  return callCommand("install_modloader_version", {
    kind,
    gameVersion,
    loaderVersion,
    name,
  });
}
