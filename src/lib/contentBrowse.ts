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

export const loaderLabels: Record<string, string> = {
  fabric: "Fabric",
  forge: "Forge",
  neoforge: "NeoForge",
  quilt: "Quilt",
};

/** Modrinth 的分类标签常用的几个翻成中文；没见过的原样显示。 */
const categoryLabels: Record<string, string> = {
  adventure: "冒险",
  cursed: "诅咒",
  decoration: "装饰",
  economy: "经济",
  equipment: "装备",
  food: "食物",
  "game-mechanics": "游戏机制",
  library: "前置库",
  magic: "魔法",
  management: "管理",
  minigame: "小游戏",
  mobs: "生物",
  optimization: "优化",
  social: "社交",
  storage: "存储",
  technology: "科技",
  transportation: "交通",
  utility: "实用",
  worldgen: "世界生成",
  realistic: "写实",
  simplistic: "简约",
  fantasy: "奇幻",
  vanilla: "原版风格",
  "vanilla-like": "原版风格",
  cartoon: "卡通",
  combat: "战斗",
  themed: "主题",
  tweaks: "微调",
  "8x-": "8x 以下",
  "16x": "16x",
  "32x": "32x",
  "64x": "64x",
  "128x": "128x",
  "256x": "256x",
  "512x+": "512x 以上",
  bloom: "泛光",
  cel: "赛璐璐",
  colored: "彩色光照",
  reflections: "反射",
  shadows: "阴影",
  "path-tracing": "路径追踪",
  potato: "低配",
  low: "低",
  medium: "中",
  high: "高",
  screenshot: "截图",
  "high-performance": "高性能",
};

export function categoryLabel(tag: string): string {
  return categoryLabels[tag] ?? tag;
}

/** `2026-10-30T10:00:00Z` → `2026-10-30`；不是这个形状就是空。 */
export function formatUpdated(iso: string): string {
  return /^\d{4}-\d{2}-\d{2}/.test(iso) ? iso.slice(0, 10) : "";
}

/** 项目支持的游戏版本范围，如 `1.16.5 – 1.21.1`；只有一个版本就是它自己。 */
export function describeVersionRange(oldest: string | null, newest: string | null): string {
  if (oldest === null && newest === null) {
    return "";
  }
  if (oldest === null || newest === null || oldest === newest) {
    return oldest ?? newest ?? "";
  }
  return `${oldest} – ${newest}`;
}

/** 客户端／服务端要求的说明，给标签用；不支持或不知道的不显示。 */
export function describeSide(label: string, side: "required" | "optional" | "unsupported" | "unknown"): string | null {
  if (side === "required") {
    return `${label}必需`;
  }
  return side === "optional" ? `${label}可选` : null;
}
