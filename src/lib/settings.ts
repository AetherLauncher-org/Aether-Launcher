import {
  clampMemoryMiB,
  isMemoryMode,
  defaultMemoryMiB,
  formatMemoryMiB,
  maxMemoryMiB,
  minMemoryMiB,
  parseStoredMemoryMiB,
} from "./memory";
import type { MemoryMode } from "./memory";

/**
 * 设置的读写：内存上限与默认 Java 运行时。
 *
 * 读写在 `localStorage` 里，和后端的实例数据分开放——这两项属于「这台机器」
 * 而不是「某个实例」。存储不可用（隐私模式、配额满）时静默回落到默认值，
 * 选择只在本次运行内生效，与 `lib/theme` 的处理一致。
 */

export const memoryStorageKey = "aether.memoryMiB";

export { javaRuntimeStorageKey } from "./javaRuntime";

export type MemoryBounds = {
  min: number;
  max: number;
  defaultValue: number;
};

export const memoryBounds: MemoryBounds = {
  min: minMemoryMiB,
  max: maxMemoryMiB,
  defaultValue: defaultMemoryMiB,
};

export function loadMemoryMiB(): number {
  try {
    return parseStoredMemoryMiB(window.localStorage.getItem(memoryStorageKey));
  } catch {
    return defaultMemoryMiB;
  }
}

export function saveMemoryMiB(value: number): number {
  const clamped = clampMemoryMiB(value);
  try {
    window.localStorage.setItem(memoryStorageKey, String(clamped));
  } catch {
    // 存储不可用时只在本次运行内生效
  }
  return clamped;
}

/** 界面上显示的一行，例如 `4 GiB / 上限 128 GiB`。 */
export function describeMemory(mib: number): string {
  return `${formatMemoryMiB(mib)} · 可调范围 ${formatMemoryMiB(
    minMemoryMiB,
  )}–${formatMemoryMiB(maxMemoryMiB)}`;
}

/**
 * 数字输入框的处理：用户边打字边清理时会出现空字符串、`-`、超长数字，
 * 这里统一收成「要么是一个被夹过的值，要么是 null（还不合法）」。
 */
export function parseMemoryInput(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return null;
  }
  const parsed = Number.parseInt(trimmed, 10);
  if (Number.isNaN(parsed)) {
    return null;
  }
  return clampMemoryMiB(parsed);
}

export function loadJavaRuntimeKey(): string | null {
  try {
    const stored = window.localStorage.getItem("aether.javaRuntime");
    return stored !== null && stored.length > 0 ? stored : null;
  } catch {
    return null;
  }
}

export function saveJavaRuntimeKey(key: string): void {
  try {
    window.localStorage.setItem("aether.javaRuntime", key);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}

const lastPlayedStorageKey = "aether.lastPlayed";

/** 最近一次启动的实例 ID，主页用它决定「继续游戏」显示哪个实例。 */
export function loadLastPlayedId(): string | null {
  try {
    const stored = window.localStorage.getItem(lastPlayedStorageKey);
    return stored !== null && stored.length > 0 ? stored : null;
  } catch {
    return null;
  }
}

export function saveLastPlayedId(id: string): void {
  try {
    window.localStorage.setItem(lastPlayedStorageKey, id);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}

export const memoryModeStorageKey = "aether.memoryMode";

/**
 * 内存模式。默认自动；但如果用户在加入模式之前已经手动存过一个值，
 * 视为自定义，不要悄悄丢掉他们设过的数字。
 */
export function loadMemoryMode(): MemoryMode {
  try {
    const stored = window.localStorage.getItem(memoryModeStorageKey);
    if (isMemoryMode(stored)) {
      return stored;
    }
    return window.localStorage.getItem(memoryStorageKey) !== null ? "custom" : "auto";
  } catch {
    return "auto";
  }
}

export function saveMemoryMode(mode: MemoryMode): void {
  try {
    window.localStorage.setItem(memoryModeStorageKey, mode);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
