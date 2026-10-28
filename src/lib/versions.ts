import type { MinecraftCatalogVersion } from "../ipc";

/**
 * 版本目录的纯逻辑：分类、搜索、可安装性判断、日期格式。
 *
 * 目录来自 Mojang 官方的 `version_manifest_v2.json`，由后端校验后返回；
 * 这里不做任何网络请求，也不信任目录里的字段超出「展示」的用途。
 */

export type VersionKind = "release" | "snapshot" | "old";

export const versionKinds: readonly VersionKind[] = ["release", "snapshot", "old"];

const kindLabels: Record<VersionKind, string> = {
  release: "正式版",
  snapshot: "快照版",
  old: "远古版",
};

export function describeKind(kind: VersionKind): string {
  return kindLabels[kind];
}

/** 把官方的四种 `type` 归并成三类；未知类型按快照处理，避免它们消失。 */
export function kindOf(version: Pick<MinecraftCatalogVersion, "type">): VersionKind {
  switch (version.type) {
    case "release":
      return "release";
    case "old_beta":
    case "old_alpha":
      return "old";
    default:
      return "snapshot";
  }
}

/**
 * 第一个使用结构化 `arguments` 的版本是 17w43a（1.13 的第一个快照）。
 * 更早的版本使用 `minecraftArguments`，启动计划暂不支持，安装了也无法启动，
 * 所以不允许安装，并在界面上说明原因。
 */
export const modernArgumentsCutoff = "2017-10-25";

export type Availability = { ok: true } | { ok: false; reason: string };

export function availability(
  version: Pick<MinecraftCatalogVersion, "type" | "releaseTime">,
): Availability {
  if (kindOf(version) === "old") {
    return { ok: false, reason: "远古版使用旧的资源与参数格式，暂不支持" };
  }
  if (version.releaseTime.slice(0, 10) < modernArgumentsCutoff) {
    return { ok: false, reason: "1.13 之前的版本使用旧的启动参数格式，暂不支持" };
  }
  return { ok: true };
}

export type VersionFilter = {
  kind: VersionKind;
  query: string;
};

/** 按类别和关键词过滤，保持目录原有的新到旧顺序。 */
export function filterVersions(
  versions: readonly MinecraftCatalogVersion[],
  filter: VersionFilter,
): MinecraftCatalogVersion[] {
  const needle = filter.query.trim().toLowerCase();
  return versions.filter(
    (version) =>
      kindOf(version) === filter.kind &&
      (needle.length === 0 || version.id.toLowerCase().includes(needle)),
  );
}

/** `2024-08-08T13:34:05+00:00` → `2024-08-08`。解析不了就原样返回。 */
export function formatReleaseDate(releaseTime: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})/.exec(releaseTime);
  return match ? match[1] : releaseTime;
}

/** `2024-08-08T13:34:05+00:00` → 本地时间的 `2024-08-08 21:34`。 */
export function formatReleaseDateTime(releaseTime: string): string {
  const parsed = new Date(releaseTime);
  if (Number.isNaN(parsed.getTime())) {
    return releaseTime;
  }
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())} ${pad(
    parsed.getHours(),
  )}:${pad(parsed.getMinutes())}`;
}

/** 目录里给出的元数据域名，用于在详情里说明来源。 */
export function metadataHost(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "未知";
  }
}

/** 默认的实例名：版本号本身。重名时追加序号。 */
export function suggestInstanceName(
  version: string,
  existingNames: readonly string[],
): string {
  const taken = new Set(existingNames.map((name) => name.toLowerCase()));
  if (!taken.has(version.toLowerCase())) {
    return version;
  }
  for (let index = 2; ; index += 1) {
    const candidate = `${version} (${index})`;
    if (!taken.has(candidate.toLowerCase())) {
      return candidate;
    }
  }
}
