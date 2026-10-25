import type { JavaRuntimeInfo } from "../ipc";

/**
 * Java 运行时列表的纯逻辑：去重、排序、标签、路径缩写，以及
 * `localStorage` 里那个「默认运行时」的选择。
 *
 * Java 选择只存在本机设置里（`aether.javaRuntime`），不经过 IPC——
 * 后端目前只是把探测到的安装列出来。
 */

/** 同一份 Java 可能被 PATH、JAVA_HOME 和常见安装目录各发现一次。 */
export function dedupeRuntimes(
  runtimes: readonly JavaRuntimeInfo[],
): JavaRuntimeInfo[] {
  const seen = new Set<string>();
  const unique: JavaRuntimeInfo[] = [];
  for (const runtime of runtimes) {
    const key = runtimeKey(runtime);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    unique.push(runtime);
  }
  return unique;
}

/** 路径作为唯一键，大小写和分隔符归一，避免 Windows 上重复。 */
export function runtimeKey(runtime: JavaRuntimeInfo): string {
  return runtime.executable.replace(/\\/g, "/").toLowerCase();
}

/** 主版本降序：新版本排前面，同版本按厂商名排。 */
export function sortRuntimes(
  runtimes: readonly JavaRuntimeInfo[],
): JavaRuntimeInfo[] {
  return [...runtimes].sort((left, right) => {
    if (right.majorVersion !== left.majorVersion) {
      return right.majorVersion - left.majorVersion;
    }
    return collator.compare(vendorLabel(left), vendorLabel(right));
  });
}

const collator = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});

/** 主版本 + 架构，用作列表里的一行标题。 */
export function describeRuntime(runtime: JavaRuntimeInfo): string {
  return `Java ${runtime.majorVersion} · ${runtime.architecture}`;
}

/** 厂商缺失时不要显示 `null`。 */
export function vendorLabel(runtime: JavaRuntimeInfo): string {
  const vendor = runtime.vendor?.trim();
  return vendor && vendor.length > 0 ? vendor : "未知厂商";
}

/**
 * 只留最后两级目录，例如 `…\jdk-21\bin\java.exe`。
 * 完整路径对用户没有额外信息量，却会把一行撑爆。
 * 保留原路径的分隔符风格，Windows 路径显示反斜杠，POSIX 路径显示斜杠。
 */
export function shortenPath(path: string, keep = 2): string {
  const useBackslash = path.includes("\\");
  const separator = useBackslash ? "\\" : "/";
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0);
  if (parts.length <= keep) {
    return parts.join(separator);
  }
  return `…${separator}${parts.slice(-keep).join(separator)}`;
}

export const javaRuntimeStorageKey = "aether.javaRuntime";

/** 从列表里挑默认选中项：优先已保存且仍存在的那一个，否则用第一个。 */
export function pickSelectedRuntime(
  runtimes: readonly JavaRuntimeInfo[],
  savedKey: string | null,
): JavaRuntimeInfo | null {
  if (runtimes.length === 0) {
    return null;
  }
  if (savedKey !== null) {
    const match = runtimes.find((runtime) => runtimeKey(runtime) === savedKey);
    if (match) {
      return match;
    }
  }
  return runtimes[0];
}

/**
 * 判断某个运行时是否是当前默认。
 * 没保存过选择时，把列表第一项当作默认（与 `pickSelectedRuntime` 一致）。
 */
export function isDefaultRuntime(
  runtime: JavaRuntimeInfo,
  runtimes: readonly JavaRuntimeInfo[],
  savedKey: string | null,
): boolean {
  const selected = pickSelectedRuntime(runtimes, savedKey);
  return selected !== null && runtimeKey(selected) === runtimeKey(runtime);
}

/** 空列表时设置页要显示的一句话。 */
export function describeEmptyRuntimes(isDesktop: boolean): string {
  return isDesktop
    ? "暂未检测到 Java。安装 Java 21 后重新扫描，或等待 Java 自动下载功能。"
    : "需要桌面版才能扫描本机的 Java 安装。";
}
