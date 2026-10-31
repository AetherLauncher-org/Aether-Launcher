import { callCommand } from "./call";
import type { InstanceSummary } from "./instances";
import { isTauri } from "./index";

/**
 * 实例管理的 IPC：详情、改名、删除、内容文件夹（模组、资源包、光影、存档、投影）、
 * 日志、打开文件夹。
 *
 * 命令定义在 `src-tauri/src/commands/manage.rs`。**前端从不传路径**：文件夹用固定的
 * 标签（`mods`、`saves`、`logs`…）指定，单个文件用纯文件名指定，后端按已校验的实例
 * 记录解析出真正的位置。日志在到达这里之前已经由后端脱敏（玩家名、令牌、邮箱、IP）。
 */

/** 实例里可以列出和管理的内容文件夹。标签与后端的 `ContentKind::label` 一一对应。 */
export type ContentKind = "mods" | "resourcepacks" | "shaderpacks" | "saves" | "schematics";

export const contentKinds: readonly ContentKind[] = [
  "mods",
  "resourcepacks",
  "shaderpacks",
  "saves",
  "schematics",
];

/** `openInstanceFolder` 能打开的位置：游戏目录、日志、崩溃报告，或某个内容文件夹。 */
export type FolderTarget = ContentKind | "game" | "logs" | "crash-reports";

export type ContentCounts = Record<ContentKind, number>;

export type InstanceDetails = {
  id: string;
  name: string;
  version: string;
  state: InstanceSummary["state"];
  /** `Fabric 0.16.5`、`Forge 52.0.1`…；原版为 null */
  loader: string | null;
  /** 实例游戏目录的大小（存档、模组、日志）；共享的游戏文件不计 */
  sizeBytes: number;
  counts: ContentCounts;
};

export type ContentItem = {
  name: string;
  sizeBytes: number;
  /** 只有被关掉的模组是 false */
  enabled: boolean;
  isFolder: boolean;
};

/** Modrinth 认得的一个文件。`name` 是磁盘上的纯文件名，没有路径。 */
export type IdentifiedContent = {
  name: string;
  platform: "modrinth";
  /** 只给 `openContentProject` 用 */
  projectSlug: string;
  title: string;
  summary: string;
  version: string;
  /** `data:` 图片（PNG、JPEG、GIF、WebP）；没有图标时为 null */
  icon: string | null;
};

export type ContentLookup = {
  /** 只含认出来的文件 */
  items: IdentifiedContent[];
  /** 没连上平台：显示的是缓存里的（或什么都没有） */
  unreachable: boolean;
};

/** 可以到 Modrinth 上识别的内容；存档和投影不在上面。 */
export type LookupKind = Extract<ContentKind, "mods" | "resourcepacks" | "shaderpacks">;

export function isLookupKind(kind: ContentKind): kind is LookupKind {
  return kind === "mods" || kind === "resourcepacks" || kind === "shaderpacks";
}

export type LogEntry = {
  kind: "latest" | "crash";
  name: string;
  sizeBytes: number;
  /** 自 Unix 纪元起的秒数 */
  modified: number | null;
};

export type LogContent = {
  /** 已脱敏的文本：日志的末尾，最多 256 KiB */
  text: string;
  /** 日志更长，开头被截掉了 */
  truncated: boolean;
};

declare module "./call" {
  interface CommandArgs {
    instance_details: { id: string };
    rename_installed_instance: { id: string; name: string };
    delete_installed_instance: { id: string };
    list_instance_content: { id: string; kind: ContentKind };
    identify_instance_content: { id: string; kind: LookupKind };
    open_content_project: { slug: string };
    set_instance_mod_enabled: { id: string; name: string; enabled: boolean };
    remove_instance_content: { id: string; kind: ContentKind; name: string };
    list_instance_logs: { id: string };
    read_instance_log: { id: string; kind: LogEntry["kind"]; name: string | null };
    open_instance_folder: { id: string; target: FolderTarget };
  }

  interface CommandResults {
    instance_details: InstanceDetails;
    rename_installed_instance: InstanceSummary;
    delete_installed_instance: null;
    list_instance_content: ContentItem[];
    identify_instance_content: ContentLookup;
    open_content_project: null;
    set_instance_mod_enabled: null;
    remove_instance_content: null;
    list_instance_logs: LogEntry[];
    read_instance_log: LogContent;
    open_instance_folder: null;
  }
}

const desktopOnly = "管理实例需要桌面版。";

function requireDesktop(): void {
  if (!isTauri()) {
    throw new Error(desktopOnly);
  }
}

export async function getInstanceDetails(id: string): Promise<InstanceDetails> {
  requireDesktop();
  return callCommand("instance_details", { id });
}

/** 只改显示名，目录名（UUID）不变。返回改名后的实例摘要。 */
export async function renameInstance(id: string, name: string): Promise<InstanceSummary> {
  requireDesktop();
  return callCommand("rename_installed_instance", { id, name });
}

/** 删除实例和它文件夹里的一切（存档、模组、设置）。共享的游戏文件保留。运行中的实例删不了。 */
export async function deleteInstance(id: string): Promise<void> {
  requireDesktop();
  await callCommand("delete_installed_instance", { id });
}

export async function listInstanceContent(id: string, kind: ContentKind): Promise<ContentItem[]> {
  requireDesktop();
  return callCommand("list_instance_content", { id, kind });
}

/**
 * 到 Modrinth 上按文件哈希识别模组、资源包或光影包，得到名称、简介、版本和图标。离开本机的
 * 只有哈希；结果在后端缓存，没网时用缓存。
 */
export async function identifyContent(id: string, kind: LookupKind): Promise<ContentLookup> {
  requireDesktop();
  return callCommand("identify_instance_content", { id, kind });
}

/** 在浏览器里打开项目页。地址由后端用 `projectSlug` 拼，前端给不了别的地址。 */
export async function openContentProject(slug: string): Promise<void> {
  requireDesktop();
  await callCommand("open_content_project", { slug });
}

/** 开关一个模组（`x.jar` ↔ `x.jar.disabled`）。 */
export async function setModEnabled(id: string, name: string, enabled: boolean): Promise<void> {
  requireDesktop();
  await callCommand("set_instance_mod_enabled", { id, name, enabled });
}

/** 删除内容文件夹里的一项；`name` 是纯文件名。 */
export async function removeContent(id: string, kind: ContentKind, name: string): Promise<void> {
  requireDesktop();
  await callCommand("remove_instance_content", { id, kind, name });
}

export async function listInstanceLogs(id: string): Promise<LogEntry[]> {
  requireDesktop();
  return callCommand("list_instance_logs", { id });
}

export async function readInstanceLog(id: string, entry: LogEntry): Promise<LogContent> {
  requireDesktop();
  return callCommand("read_instance_log", {
    id,
    kind: entry.kind,
    name: entry.kind === "crash" ? entry.name : null,
  });
}

export async function openInstanceFolder(id: string, target: FolderTarget): Promise<void> {
  requireDesktop();
  await callCommand("open_instance_folder", { id, target });
}
