import type { SystemMemory } from "../ipc";

/**
 * 实例页与「设置 → 内存」共用的数值范围。
 *
 * 512 MiB – 128 GiB 与 `aether-launch` 里 `LaunchRequest::validate` 的检查一致。
 * 前端先夹取一次，后端仍然会再校验一遍。
 */

export const defaultMemoryMiB = 4096;

export const minMemoryMiB = 512;

export const maxMemoryMiB = 131_072;

/** 滑块步长。用 256 的倍数，拖动时不必逐 MiB 调整。 */
export const memorySliderStep = 256;

/** 把任意输入夹到 512–131072 之间，非法或缺失时回落到默认值。 */
export function clampMemoryMiB(value: number | null | undefined): number {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return defaultMemoryMiB;
  }
  const rounded = Math.round(value);
  if (rounded < minMemoryMiB) {
    return minMemoryMiB;
  }
  if (rounded > maxMemoryMiB) {
    return maxMemoryMiB;
  }
  return rounded;
}

/** 2048 → `2 GiB`；1024 以下保留 MiB，方便在徽章和提示里读。 */
export function formatMemoryMiB(mib: number): string {
  const clamped = clampMemoryMiB(mib);
  if (clamped >= 1024) {
    const gib = clamped / 1024;
    const text = Number.isInteger(gib) ? gib.toFixed(0) : gib.toFixed(1);
    return `${text} GiB`;
  }
  return `${clamped} MiB`;
}

/** 解析 `localStorage` 里的字符串，越界或损坏时回落到默认值。 */
export function parseStoredMemoryMiB(raw: string | null): number {
  if (raw === null) {
    return defaultMemoryMiB;
  }
  const parsed = Number.parseInt(raw, 10);
  return clampMemoryMiB(Number.isNaN(parsed) ? undefined : parsed);
}

/**
 * 内存设置的两种方式：`auto` 由启动器按本机内存推荐，`custom` 用用户拖出来的值。
 */
export type MemoryMode = "auto" | "custom";

export function isMemoryMode(value: unknown): value is MemoryMode {
  return value === "auto" || value === "custom";
}

/** 读不到本机内存时，滑块的上限按 16 GiB 估计。 */
export const fallbackSliderMaxMiB = 16_384;

/** 不做夹取的格式化：显示「可用 300 MiB」这类小于下限的数字时不能被抬到 512。 */
export function formatMiB(mib: number): string {
  const value = Math.max(0, Math.round(mib));
  if (value >= 1024) {
    const gib = value / 1024;
    return `${Number.isInteger(gib) ? gib.toFixed(0) : gib.toFixed(1)} GiB`;
  }
  return `${value} MiB`;
}

function floorToStep(value: number): number {
  return Math.floor(value / memorySliderStep) * memorySliderStep;
}

/**
 * 滑块的上限：本机安装的物理内存，按步长向下取整，且不超过后端接受的最大值。
 * 读不到本机内存时用估计值。至少比下限大一格，保证滑块有可拖的范围。
 */
export function sliderMaxMiB(system: SystemMemory | null): number {
  const raw = system === null ? fallbackSliderMaxMiB : floorToStep(system.totalMib);
  return Math.max(minMemoryMiB + memorySliderStep, Math.min(raw, maxMemoryMiB));
}

/** 其他程序已经占用的内存；系统没有报告可用内存时为 null。 */
export function usedMiB(system: SystemMemory | null): number | null {
  if (system === null || system.availableMib === null) {
    return null;
  }
  return Math.max(0, system.totalMib - system.availableMib);
}

/**
 * 自动模式的推荐值：可用内存的 65%，不超过 8 GiB 也不超过总内存的 75%，
 * 不低于 2 GiB（总内存很小时让位于 75% 的上限）。读不到本机内存时用默认值。
 */
export function recommendedMemoryMiB(system: SystemMemory | null): number {
  if (system === null) {
    return defaultMemoryMiB;
  }
  const cap = floorToStep(system.totalMib * 0.75);
  const available = system.availableMib ?? system.totalMib * 0.6;
  const wanted = Math.min(8192, available * 0.65);
  const floor = Math.min(2048, cap);
  const value = Math.max(floor, Math.min(floorToStep(wanted), cap));
  return clampMemoryMiB(value);
}

/** 启动时实际使用的内存上限。 */
export function effectiveMemoryMiB(
  mode: MemoryMode,
  customMiB: number,
  system: SystemMemory | null,
): number {
  return mode === "auto" ? recommendedMemoryMiB(system) : clampMemoryMiB(customMiB);
}

/** 设置的值是否超过了当前可用内存；不知道可用内存时返回 null（不下结论）。 */
export function exceedsAvailable(valueMiB: number, system: SystemMemory | null): boolean | null {
  if (system === null || system.availableMib === null) {
    return null;
  }
  return valueMiB > system.availableMib;
}

/** 实例卡片等处的简短说明：自动模式不报具体数字，因为它每次启动都可能不同。 */
export function describeMemorySetting(mode: MemoryMode, customMiB: number): string {
  return mode === "auto" ? "自动" : formatMemoryMiB(customMiB);
}
