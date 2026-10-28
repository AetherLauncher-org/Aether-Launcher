import type { SystemMemory } from "../ipc";
import { memorySliderStep, minMemoryMiB, sliderMaxMiB, usedMiB } from "./memory";

/**
 * 内存滑块的几何：一个分配值，两个手柄，两个阶段。
 *
 * 轨道是整台电脑的物理内存：左边是系统已用（红），分界线右边是可用内存。
 * - 阶段一：分配值不超过可用内存。右手柄在可用区里移动，绿色从分界线铺到手柄。
 * - 阶段二：右手柄推到底（「满」，即把可用内存全部分配）之后，左边的分界线上
 *   出现第二个手柄，可以继续往左拖进红区——分配值超过了可用内存，那一段是橙色，
 *   意味着会把内存挤爆。
 * 两个手柄控制的是同一个值，所以键盘和读屏也只需要一个滑块。
 */

export type BarGeometry = {
  /** 轨道代表的总内存 */
  totalMib: number;
  /** 其他程序已用的内存；不知道时为 null，此时没有红区和第二个手柄 */
  usedMib: number | null;
  usedPct: number;
  /** 「满」：把可用内存全部分配（按步长取整） */
  fullMib: number;
  /** 分配值，已夹到可调范围内 */
  valueMiB: number;
  /** 超出可用内存的部分 */
  overMiB: number;
  rightPct: number;
  /** 第二个手柄的位置；分配值还没到「满」时为 null */
  leftPct: number | null;
};

function floorToStep(value: number): number {
  return Math.floor(value / memorySliderStep) * memorySliderStep;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

export function barGeometry(valueMiB: number, system: SystemMemory | null): BarGeometry {
  const max = sliderMaxMiB(system);
  const totalMib = system?.totalMib ?? max;
  const used = usedMiB(system);
  const pct = (mib: number) => clamp((mib / totalMib) * 100, 0, 100);

  const value = clamp(valueMiB, minMemoryMiB, max);
  const usedPct = used === null ? 0 : pct(used);
  const fullMib =
    used === null ? max : clamp(floorToStep(totalMib - used), minMemoryMiB, max);
  const overMiB = used === null ? 0 : Math.max(0, value - fullMib);

  return {
    totalMib,
    usedMib: used,
    usedPct,
    fullMib,
    valueMiB: value,
    overMiB,
    rightPct: used === null ? pct(value) : clamp(usedPct + pct(Math.min(value, fullMib)), 0, 100),
    leftPct: used === null || value < fullMib ? null : clamp(usedPct - pct(overMiB), 0, 100),
  };
}

function roundToStep(value: number): number {
  return Math.round(value / memorySliderStep) * memorySliderStep;
}

/** 拖右手柄：位置（轨道上的百分比）换成分配值，只在可用区里取值。 */
export function valueFromRightThumb(pct: number, system: SystemMemory | null): number {
  const geometry = barGeometry(minMemoryMiB, system);
  const fromDivider = ((pct - geometry.usedPct) / 100) * geometry.totalMib;
  return clamp(roundToStep(fromDivider), minMemoryMiB, geometry.fullMib);
}

/** 拖左手柄：位置换成分配值，从「满」开始往红区里加。 */
export function valueFromLeftThumb(pct: number, system: SystemMemory | null): number {
  const geometry = barGeometry(minMemoryMiB, system);
  const max = sliderMaxMiB(system);
  const over = ((geometry.usedPct - pct) / 100) * geometry.totalMib;
  return clamp(geometry.fullMib + roundToStep(over), geometry.fullMib, max);
}

/**
 * 点在轨道上（不是手柄）时该动哪个手柄：点在红区里只有在已经「满」的时候才有意义，
 * 否则红区不可滑动，返回 null。
 */
export function thumbForTrackPress(
  pct: number,
  geometry: BarGeometry,
): "right" | "left" | null {
  if (geometry.usedMib === null || pct >= geometry.usedPct) {
    return "right";
  }
  return geometry.leftPct !== null ? "left" : null;
}
