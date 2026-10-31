import type { ContentTarget, ContentType, SearchSort } from "../ipc";

export const contentTypeLabels: Record<ContentType, string> = {
  mod: "模组",
  datapack: "数据包",
  resourcepack: "资源包",
  shader: "光影",
};

export const sortLabels: Record<SearchSort, string> = {
  relevance: "相关度",
  downloads: "下载量",
  newest: "最新发布",
  updated: "最近更新",
};

export const searchSorts: readonly SearchSort[] = ["relevance", "downloads", "newest", "updated"];

const compact = new Intl.NumberFormat("zh-CN", { notation: "compact", maximumFractionDigits: 1 });

/** `1234567` → `123万`；给搜索结果里的下载量用。 */
export function formatDownloads(count: number): string {
  return Number.isFinite(count) && count >= 0 ? compact.format(count) : "—";
}

/** 实例装得了什么：给筛选条旁边的说明。 */
export function describeTarget(target: ContentTarget): string {
  const game = target.gameVersion ?? "未知版本";
  const loaders: Record<string, string> = { fabric: "Fabric", quilt: "Quilt", forge: "Forge", neoforge: "NeoForge" };
  return target.loader === null ? game : `${game} · ${loaders[target.loader] ?? target.loader}`;
}

/** 模组需要加载器，原版实例装不了；其余类型不挑。返回不能装的原因，能装就是 null。 */
export function cannotInstallReason(type: ContentType, target: ContentTarget | null): string | null {
  if (target === null) {
    return null;
  }
  if (target.gameVersion === null) {
    return "看不出这个实例的游戏版本";
  }
  if (type === "mod" && target.loader === null) {
    return "这个实例没有模组加载器，模组需要 Fabric、Quilt、Forge 或 NeoForge";
  }
  return null;
}
