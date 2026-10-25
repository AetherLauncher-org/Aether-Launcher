/**
 * 资源库页的纯逻辑：资源条目、搜索过滤、排序、来源聚合。
 *
 * 资源库目前没有后端数据——下载计划与来源校验都还没接上。这里的类型和函数
 * 是为那一天准备的真实结构，**不要**拿假数据填充页面：列表为空是有意的结果，
 * 见 `LibraryPage` 的空状态。
 */

export type ResourceKind = "mod" | "resourcepack" | "shader" | "datapack";

export type ResourceEntry = {
  /** 稳定标识，通常来自来源站点的项目 ID */
  id: string;
  name: string;
  /** 适合哪个 Minecraft 版本 */
  gameVersion: string;
  kind: ResourceKind;
  /** 提供方，例如 `modrinth`、`curseforge` */
  source: string;
  /** 下载地址的域名，界面上只显示域名，不显示完整 URL */
  host: string;
  /** 文件的 SHA-1，来源校验完成后才有 */
  sha1: string | null;
  /** 字节数 */
  size: number;
};

const kindLabels: Record<ResourceKind, string> = {
  mod: "模组",
  resourcepack: "资源包",
  shader: "光影",
  datapack: "数据包",
};

export function describeResourceKind(kind: ResourceKind): string {
  return kindLabels[kind];
}

/** 来源过滤器的一选项：来源名 + 它下面有多少条目。 */
export type SourceFacet = {
  source: string;
  count: number;
};

const collator = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});

/**
 * 聚合来源，按条目数降序、同数量按名称排。
 * 过滤器的选项直接来自这里，避免写死一份来源列表和后端数据对不上。
 */
export function aggregateSources(
  entries: readonly ResourceEntry[],
): SourceFacet[] {
  const counts = new Map<string, number>();
  for (const entry of entries) {
    counts.set(entry.source, (counts.get(entry.source) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([source, count]) => ({ source, count }))
    .sort((left, right) => {
      if (right.count !== left.count) {
        return right.count - left.count;
      }
      return collator.compare(left.source, right.source);
    });
}

export type LibraryFilters = {
  query: string;
  /** `null` 表示「全部来源」 */
  source: string | null;
};

/** 先按来源，再按关键词（名称、版本、提供方）。 */
export function filterResources(
  entries: readonly ResourceEntry[],
  filters: LibraryFilters,
): ResourceEntry[] {
  const needle = filters.query.trim().toLowerCase();
  return entries.filter((entry) => {
    if (filters.source !== null && entry.source !== filters.source) {
      return false;
    }
    if (needle.length === 0) {
      return true;
    }
    return (
      entry.name.toLowerCase().includes(needle) ||
      entry.gameVersion.toLowerCase().includes(needle) ||
      entry.source.toLowerCase().includes(needle)
    );
  });
}

export type ResourceSortKey = "name" | "size" | "gameVersion";

/** 名称按拼音、版本按数值段、大小按字节；同值时按名称兜底。 */
export function sortResources(
  entries: readonly ResourceEntry[],
  key: ResourceSortKey,
): ResourceEntry[] {
  return [...entries].sort((left, right) => {
    const primary = compareBy(key, left, right);
    if (primary !== 0) {
      return primary;
    }
    return collator.compare(left.name, right.name);
  });
}

function compareBy(
  key: ResourceSortKey,
  left: ResourceEntry,
  right: ResourceEntry,
): number {
  switch (key) {
    case "size":
      return left.size - right.size;
    case "gameVersion":
      return compareVersions(left.gameVersion, right.gameVersion);
    case "name":
      return collator.compare(left.name, right.name);
  }
}

/**
 * 版本号按段比较：`1.21.1` 大于 `1.20.10`，缺段按 0 处理。
 *
 * 预发布后缀（`1.20.5-pre1`）排在正式版 `1.20.5` 之前——这是 Minecraft
 * 快照/预发布的惯例，也与 `aether-manifest` 的版本比较一致。
 */
export function compareVersions(left: string, right: string): number {
  const leftParts = splitVersion(left);
  const rightParts = splitVersion(right);
  const numericLength = Math.max(
    leftParts.numeric.length,
    rightParts.numeric.length,
  );

  for (let index = 0; index < numericLength; index += 1) {
    const a = leftParts.numeric[index] ?? 0;
    const b = rightParts.numeric[index] ?? 0;
    if (a !== b) {
      return a - b;
    }
  }

  // 数字段一样时，带后缀的那个是预发布，排在前面
  if (leftParts.suffix === rightParts.suffix) {
    return 0;
  }
  if (leftParts.suffix.length === 0) {
    return 1;
  }
  if (rightParts.suffix.length === 0) {
    return -1;
  }
  return collator.compare(leftParts.suffix, rightParts.suffix);
}

/** 拆成数字段与前缀/后缀文本。`1.20.5-pre1` → 数字 `[1,20,5]`、后缀 `pre1`。 */
function splitVersion(
  version: string,
): { numeric: number[]; suffix: string } {
  const [core, ...rest] = version.split(/[-+]/);
  const numeric = core
    .split(".")
    .map((part) => Number.parseInt(part, 10))
    .filter((part) => !Number.isNaN(part));
  return { numeric, suffix: rest.join("-") };
}

/** 文件大小：1536 → `1.5 KB`。资源包动辄几百 MB，界面上统一用 KB/MB/GB。 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "未知大小";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"] as const;
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const rounded = value >= 100 ? value.toFixed(0) : value.toFixed(1);
  return `${rounded} ${units[unitIndex]}`;
}

/** 摘要：`共 12 项 · 3 个来源`，空列表时单独给一句话。 */
export function summariseResources(entries: readonly ResourceEntry[]): string {
  if (entries.length === 0) {
    return "还没有可用的资源";
  }
  const sources = aggregateSources(entries).length;
  return `共 ${entries.length} 项 · ${sources} 个来源`;
}
