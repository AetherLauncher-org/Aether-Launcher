import type { ContentItem, ContentKind, LogEntry } from "../ipc/manage";

/**
 * 实例管理页的纯逻辑：每种内容的文案、文件大小和时间的显示、列表项的显示名。
 * 页面只负责组装，判断都在这里，方便直接单测。
 */

export type ContentCopy = {
  /** 标签页上的名字 */
  tab: string;
  /** 「还没有…」的标题 */
  emptyTitle: string;
  emptyBody: string;
  /** 打开文件夹的按钮文字 */
  openLabel: string;
  /** 删除确认里怎么称呼一项 */
  noun: string;
};

export const contentCopy: Record<ContentKind, ContentCopy> = {
  mods: {
    tab: "模组",
    emptyTitle: "还没有模组",
    emptyBody: "把 .jar 模组文件放进模组文件夹，需要先给这个实例装上 Forge、NeoForge 或 Fabric。",
    openLabel: "打开模组文件夹",
    noun: "模组",
  },
  resourcepacks: {
    tab: "资源包",
    emptyTitle: "还没有资源包",
    emptyBody: "把 .zip 资源包放进资源包文件夹，进游戏后在「资源包」里启用。",
    openLabel: "打开资源包文件夹",
    noun: "资源包",
  },
  shaderpacks: {
    tab: "光影",
    emptyTitle: "还没有光影包",
    emptyBody: "把 .zip 光影包放进光影文件夹。需要 Iris 或 OptiFine 这类光影加载器才能使用。",
    openLabel: "打开光影文件夹",
    noun: "光影包",
  },
  saves: {
    tab: "存档",
    emptyTitle: "还没有存档",
    emptyBody: "进游戏创建世界后，存档会出现在这里。",
    openLabel: "打开存档文件夹",
    noun: "存档",
  },
  schematics: {
    tab: "投影",
    emptyTitle: "还没有投影文件",
    emptyBody: "把 Litematica 的 .litematic（或 .schem、.nbt）文件放进投影文件夹。",
    openLabel: "打开投影文件夹",
    noun: "投影文件",
  },
};

const units = ["B", "KB", "MB", "GB", "TB"] as const;

/** `1536` → `1.5 KB`。1000 以下不带小数；到 GB 以上保留一位。 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "—";
  }
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = unit === 0 || value >= 100 ? Math.round(value).toString() : value.toFixed(1);
  return `${text} ${units[unit]}`;
}

/** 秒数（Unix 纪元）→ `2026-10-30 18:24`；没有时间为空。 */
export function formatModified(seconds: number | null): string {
  if (seconds === null) {
    return "";
  }
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  const two = (value: number) => value.toString().padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())} ${two(date.getHours())}:${two(date.getMinutes())}`;
}

const disabledSuffix = ".disabled";

/** 列表里显示的名字：模组去掉 `.jar` 和 `.disabled`，资源包、光影去掉 `.zip`，其余原样。 */
export function displayName(kind: ContentKind, item: Pick<ContentItem, "name" | "isFolder">): string {
  if (item.isFolder) {
    return item.name;
  }
  let name = item.name;
  if (kind === "mods") {
    name = name.endsWith(disabledSuffix) ? name.slice(0, -disabledSuffix.length) : name;
    return name.replace(/\.jar$/i, "");
  }
  if (kind === "resourcepacks" || kind === "shaderpacks") {
    return name.replace(/\.zip$/i, "");
  }
  return name;
}

/** 日志列表里的一行：最新日志，或崩溃报告（带时间）。 */
export function describeLog(entry: LogEntry): string {
  if (entry.kind === "latest") {
    return "最近一次游戏日志（latest.log）";
  }
  const when = formatModified(entry.modified);
  return when === "" ? entry.name : `崩溃报告 ${when}`;
}

/** 删除确认里的一句话。世界是文件夹，要把「里面的一切」说清楚。 */
export function describeRemoval(kind: ContentKind, item: ContentItem): string {
  const name = displayName(kind, item);
  if (kind === "saves") {
    return `「${name}」会连同里面的所有区块、玩家数据和备份一起删除，无法恢复。`;
  }
  return `「${name}」会从实例里删除，无法恢复。`;
}

/** 这一项是什么形态：存档和解开的包是文件夹，其余是压缩包或 jar。 */
export function describeShape(kind: ContentKind, item: Pick<ContentItem, "isFolder">): string {
  if (item.isFolder) {
    return kind === "saves" ? "世界" : "文件夹";
  }
  return kind === "mods" ? "JAR" : "压缩包";
}
