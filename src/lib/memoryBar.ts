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
  /**
   * 橙色手柄的位置：永远在分界线上或分界线左边（分配值越界越多，越往左）。
   * 不知道已用内存时没有分界线，也就没有这个手柄，为 null。
   */
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
    leftPct: used === null ? null : clamp(usedPct - pct(overMiB), 0, 100),
  };
}

/**
 * 指针拖动默认吸附的粒度：512 MiB 的整数倍，和最小值 512 MiB 在同一条网格上。
 * 按住 Shift 拖动时关闭吸附，改用下面的细粒度。
 */
export const snapStepMiB = 512;

/**
 * 不吸附（按住 Shift）时指针拖动的取整粒度。比键盘和设置里用的 256 MiB 细得多：
 * 轨道上 1 像素大约是十几到几十 MiB，用 256 的话手柄会一格一格地跳，不跟手。
 * 32 MiB 约等于 1–2 个像素，看上去是连续的；落到设置里的值仍然是合法的 MiB 整数。
 */
export const pointerStepMiB = 32;

function roundToStep(value: number, fine: boolean): number {
  const step = fine ? pointerStepMiB : snapStepMiB;
  return Math.round(value / step) * step;
}

/**
 * 拖右手柄：位置（轨道上的百分比）换成分配值，只在可用区里取值。
 * `fine` 为 true（按住 Shift）时不吸附到 512 MiB。
 */
export function valueFromRightThumb(
  pct: number,
  system: SystemMemory | null,
  fine = false,
): number {
  const geometry = barGeometry(minMemoryMiB, system);
  const fromDivider = ((pct - geometry.usedPct) / 100) * geometry.totalMib;
  return clamp(roundToStep(fromDivider, fine), minMemoryMiB, geometry.fullMib);
}

/** 拖左手柄：位置换成分配值，从「满」开始往红区里加。吸附的是最终的分配值。 */
export function valueFromLeftThumb(
  pct: number,
  system: SystemMemory | null,
  fine = false,
): number {
  const geometry = barGeometry(minMemoryMiB, system);
  const max = sliderMaxMiB(system);
  const over = ((geometry.usedPct - pct) / 100) * geometry.totalMib;
  return clamp(roundToStep(geometry.fullMib + over, fine), geometry.fullMib, max);
}

/**
 * 拖任何一个手柄：都是相对的，手柄不会在按下的瞬间跳到指针下面，而是保持按下时的偏移。
 * `increasePct` 是「多分配」的方向上指针移动的百分比（负数是少分配）：蓝色手柄往右、
 * 橙色手柄往左都是多分配。位置全部由同一个分配值算出来，所以顺序是自然的——
 * 蓝色没到底时多出来的部分让蓝色往右增加；蓝色到底之后继续多分配，橙色才往左走进红区；
 * 反方向则是橙色先退回分界线，再往下蓝色才减少。
 *
 * 结果默认吸附到 512 MiB 的整数倍（最小值和上限本身仍可到达）；`fine` 为 true
 * （按住 Shift）时只取整到 32 MiB。
 */
export function valueFromDrag(
  startValueMiB: number,
  increasePct: number,
  system: SystemMemory | null,
  fine = false,
): number {
  const geometry = barGeometry(minMemoryMiB, system);
  const max = sliderMaxMiB(system);
  return clamp(
    roundToStep(startValueMiB + (increasePct / 100) * geometry.totalMib, fine),
    minMemoryMiB,
    max,
  );
}

/** 轨道上的一根刻度：位置是轨道上的百分比，`valueMiB` 是它代表的分配值。 */
export type MemoryTick = { valueMiB: number; pct: number; major: boolean };

/** 刻度最多这么多根，再多在窄轨道上就糊成一片。 */
const maxTicks = 32;

/**
 * 吸附网格对应的刻度：分配值每 512 MiB 一根，位置和手柄完全一致——可用区里从分界线
 * 往右，越过「满」之后在红区里从分界线往左。内存很大时刻度间隔翻倍（1 GiB、2 GiB…），
 * 保证不超过 32 根；每个整 GiB（间隔翻倍后是每两格）画得更长一些。
 */
export function memoryTicks(system: SystemMemory | null): MemoryTick[] {
  const geometry = barGeometry(minMemoryMiB, system);
  const max = sliderMaxMiB(system);
  let step = snapStepMiB;
  while ((max - minMemoryMiB) / step > maxTicks) {
    step *= 2;
  }
  const majorEvery = step === snapStepMiB ? 1024 : step * 2;

  const ticks: MemoryTick[] = [];
  for (let value = minMemoryMiB; value <= max; value += step) {
    const beyondFull = geometry.usedMib !== null && value > geometry.fullMib;
    const pct = beyondFull
      ? geometry.usedPct - ((value - geometry.fullMib) / geometry.totalMib) * 100
      : geometry.usedPct + (value / geometry.totalMib) * 100;
    if (pct < 0 || pct > 100) {
      continue;
    }
    ticks.push({ valueMiB: value, pct, major: value % majorEvery === 0 });
  }
  return ticks;
}

/**
 * 点在轨道上（不是手柄）时该动哪个手柄：点在分界线左边（红区）就是想直接把橙色手柄
 * 放到那里；其余位置动蓝色手柄。不知道已用内存时只有蓝色手柄。
 */
export function thumbForTrackPress(pct: number, geometry: BarGeometry): "right" | "left" {
  return geometry.usedMib !== null && pct < geometry.usedPct ? "left" : "right";
}
