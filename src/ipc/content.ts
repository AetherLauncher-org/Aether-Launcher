import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 找内容和装内容的 IPC：从 Modrinth 搜索模组、数据包、资源包、光影，装进实例（数据包装进
 * 某个世界）。命令定义在 `src-tauri/src/commands/content.rs`，规则在 `aether_app::content_install`。
 *
 * **前端从不传路径或下载地址**：实例用 id、项目用 Modrinth 的项目 id、世界用文件夹名指定；
 * 文件落在哪个文件夹由内容类型和后端校验过的实例记录决定，下载地址取自 Modrinth 的回答并
 * 必须在它自己的 CDN 上，文件按 Modrinth 给的 SHA-1 / SHA-512 和大小校验，不符就丢掉，已有
 * 同名文件不覆盖。
 */

/** 能搜索的内容类型，Modrinth 的 `project_type`。整合包只能搜，现在还不能装。 */
export type ContentType = "mod" | "datapack" | "resourcepack" | "shader";

export const contentTypes: readonly ContentType[] = ["mod", "datapack", "resourcepack", "shader"];

export type SearchSort = "relevance" | "downloads" | "newest" | "updated";

/** 一个实例能装什么：游戏版本和模组加载器，用来给搜索设默认筛选。 */
export type ContentTarget = {
  /** `1.21.1`；版本 id 看不出来时为 null */
  gameVersion: string | null;
  /** `fabric` / `quilt` / `forge` / `neoforge`；原版为 null */
  loader: string | null;
};

export type SearchHit = {
  projectId: string;
  /** 只用来打开项目页（`openContentProject`） */
  slug: string;
  title: string;
  summary: string;
  author: string;
  downloads: number;
  projectType: string;
};

export type SearchResult = {
  hits: SearchHit[];
  total: number;
};

export type InstalledFile = {
  projectId: string;
  /** 落在文件夹里的纯文件名 */
  fileName: string;
  version: string;
};

export type SkippedProject = {
  projectId: string;
  reason: string;
};

export type InstallReport = {
  installed: InstalledFile[];
  /** 没能装上的必需依赖和原因；项目本身失败是抛错，不在这里 */
  skipped: SkippedProject[];
};

declare module "./call" {
  interface CommandArgs {
    get_content_target: { id: string };
    search_content: {
      text: string;
      projectType: ContentType;
      gameVersion: string | null;
      loader: string | null;
      sort: SearchSort;
      offset: number;
    };
    install_content: {
      id: string;
      projectId: string;
      projectType: ContentType;
      world: string | null;
      withDependencies: boolean;
    };
  }

  interface CommandResults {
    get_content_target: ContentTarget;
    search_content: SearchResult;
    install_content: InstallReport;
  }
}

function requireDesktop(): void {
  if (!isTauri()) {
    throw new Error("下载内容需要桌面版。");
  }
}

export async function getContentTarget(id: string): Promise<ContentTarget> {
  requireDesktop();
  return callCommand("get_content_target", { id });
}

export async function searchContent(
  text: string,
  projectType: ContentType,
  gameVersion: string | null,
  loader: string | null,
  sort: SearchSort,
  offset: number,
): Promise<SearchResult> {
  requireDesktop();
  return callCommand("search_content", { text, projectType, gameVersion, loader, sort, offset });
}

/** 装一个项目的最新合适版本；`world` 只有数据包用。运行中的实例装不了。 */
export async function installContent(
  id: string,
  projectId: string,
  projectType: ContentType,
  world: string | null,
  withDependencies: boolean,
): Promise<InstallReport> {
  requireDesktop();
  return callCommand("install_content", { id, projectId, projectType, world, withDependencies });
}
