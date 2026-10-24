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
