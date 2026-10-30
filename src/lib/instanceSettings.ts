import { clampMemoryMiB, parseStoredMemoryMiB } from "./memory";
import { resolveLaunchMemoryMiB } from "./launchMemory";

/**
 * 单个实例的启动设置覆盖：内存。
 *
 * 和全局的内存设置一样存在 `localStorage`（属于这台机器，不进实例记录），键里带实例
 * ID。没设过就是「跟随全局」：按设置页的自动或自定义。存储不可用时静默回落到跟随全局，
 * 选择只在本次运行内生效。
 *
 * Java 暂时没有单实例覆盖：后端现在按游戏要求的主版本自己挑 Java，前端没有任何传
 * Java 的通道（设计要求前端不传路径），见 `docs/modules/instance-management.md` 的待定。
 */

export type InstanceSettings = {
  /** 内存上限（MiB）；null 表示跟随全局设置 */
  memoryMiB: number | null;
};

export const noOverrides: InstanceSettings = { memoryMiB: null };

export function instanceSettingsKey(id: string): string {
  return `aether.instance.${id}`;
}

export function loadInstanceSettings(id: string): InstanceSettings {
  try {
    const raw = window.localStorage.getItem(instanceSettingsKey(id));
    if (raw === null) {
      return noOverrides;
    }
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) {
      return noOverrides;
    }
    const { memoryMiB } = parsed as Record<string, unknown>;
    return {
      memoryMiB:
        typeof memoryMiB === "number" && Number.isFinite(memoryMiB)
          ? parseStoredMemoryMiB(String(memoryMiB))
          : null,
    };
  } catch {
    return noOverrides;
  }
}

/** 跟随全局时直接删掉这条记录，不留空壳。 */
export function saveInstanceSettings(id: string, settings: InstanceSettings): InstanceSettings {
  const clean: InstanceSettings = {
    memoryMiB: settings.memoryMiB === null ? null : clampMemoryMiB(settings.memoryMiB),
  };
  try {
    if (clean.memoryMiB === null) {
      window.localStorage.removeItem(instanceSettingsKey(id));
    } else {
      window.localStorage.setItem(instanceSettingsKey(id), JSON.stringify(clean));
    }
  } catch {
    // 存储不可用时只在本次运行内生效
  }
  return clean;
}

/** 实例删除后清掉它的覆盖。 */
export function clearInstanceSettings(id: string): void {
  try {
    window.localStorage.removeItem(instanceSettingsKey(id));
  } catch {
    // 存储不可用时没有什么可清的
  }
}

/** 启动一个实例用的内存：实例自己的覆盖优先，否则按全局设置。 */
export async function resolveLaunchSettings(id: string): Promise<{ memoryMiB: number }> {
  const overrides = loadInstanceSettings(id);
  return { memoryMiB: overrides.memoryMiB ?? (await resolveLaunchMemoryMiB()) };
}
