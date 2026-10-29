import type { SystemMemory } from "../ipc";
import { minMemoryMiB, sliderMaxMiB, usedMiB } from "./memory";

/**
 * 内存滑块的几何：**一条分配值轴、一个手柄**。
 *
 * 轨道从左到右是 `0 MiB` 到这台机器的物理内存。手柄放在 `分配值 / total` 处，
 * 所以往右拖永远是多分配、往左拖永远是少分配——和任何一个普通滑块一样。
 *
 * 轨道上画两段色，**从头（左）开始，互不重叠**：
 *
 * - **蓝色**：从最左端铺到手柄，就是分配给游戏的那一段；
 * - **红色**：系统已经占掉的，从**最右端往左**铺。
 *
 * 两段各有各的尺子，谁也不占谁的位置。手柄还没走进红区时中间是空的（可用内存）；
 * 一旦手柄越过分界线，红色就直接顶在手柄右边——「再往右就要抢别人的内存了」。
 *
 * 早先的版本把系统已用画在左边、又给越界单独加了一个往左跑的手柄，于是「多分配」
 * 在轨道上有两个相反的方向。这里只留一个手柄，越界只体现为红区贴上来 + 变橙。
 */

export type BarGeometry = {
  /** 轨道代表的总内存 */
  totalMib: number;
  /** 其他程序已用的内存；不知道时为 null，此时没有红区 */
  usedMib: number | null;
  /** 手柄位置：分配值占轨道的百分比 */
  thumbPct: number;
  /** 红区左边缘的位置：从最右端往左铺这么宽 */
  usedStartPct: number;
  /** 分配值，已夹到可调范围内 */
  valueMiB: number;
  /** 超出可用内存的部分；不知道已用内存时为 0 */
  overMiB: number;
  /** 手柄是否已经压进红区（分配超过可用内存） */
  over: boolean;
};

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

export function barGeometry(valueMiB: number, system: SystemMemory | null): BarGeometry {
  const max = sliderMaxMiB(system);
  const totalMib = system?.totalMib ?? max;
  const used = usedMiB(system);
  const pct = (mib: number) => clamp((mib / totalMib) * 100, 0, 100);

  const value = clamp(valueMiB, minMemoryMiB, max);
  // 还能再分多少：只用来看「越界」与否，不做分阶段取整
  const freeMib = used === null ? totalMib : Math.max(0, totalMib - used);
  const overMiB = used === null ? 0 : Math.max(0, value - freeMib);

  return {
    totalMib,
    usedMib: used,
    thumbPct: pct(value),
    // 红区从右端往左铺 usedMib，所以左边缘在 100 - usedPct 处。
    usedStartPct: used === null ? 100 : 100 - pct(used),
    valueMiB: value,
    overMiB,
    over: overMiB > 0,
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
 * 轨道上的某个位置对应多少内存。
 *
 * 只有一个手柄、一条轴，所以位置和值一一对应：点哪儿就是哪儿。
 */
export function valueFromPosition(
  pct: number,
  system: SystemMemory | null,
  fine = false,
): number {
  const max = sliderMaxMiB(system);
  const totalMib = system?.totalMib ?? max;
  const raw = (clamp(pct, 0, 100) / 100) * totalMib;
  return clamp(roundToStep(raw, fine), minMemoryMiB, max);
}

/**
 * 拖动手柄：相对的，按下时手柄不跳到指针底下，而是保持按下的偏移。
 *
 * `movedPct` 是指针相对按下点移动了多少个轨道百分比，往右为正。位置和值一一对应，
 * 所以就是「原值 + 这一段对应的内存」，没有任何分阶段的分支。
 */
export function valueFromDrag(
  startValueMiB: number,
  movedPct: number,
  system: SystemMemory | null,
  fine = false,
): number {
  const max = sliderMaxMiB(system);
  const totalMib = system?.totalMib ?? max;
  return clamp(
    roundToStep(startValueMiB + (movedPct / 100) * totalMib, fine),
    minMemoryMiB,
    max,
  );
}

/** 轨道上的一根刻度：位置是轨道上的百分比，`valueMiB` 是它代表的内存。 */
export type MemoryTick = { valueMiB: number; pct: number; major: boolean };

/** 刻度最多这么多根，再多在窄轨道上就糊成一片。 */
const maxTicks = 32;

/**
 * 吸附网格对应的刻度：每 512 MiB 一根，位置和手柄用的是同一把尺子，所以刻度就是
 * 手柄会停下的那些地方。内存很大时刻度间隔翻倍（1 GiB、2 GiB…），保证不超过 32 根；
 * 每个整 GiB（间隔翻倍后是每两格）画得更长一些。
 */
export function memoryTicks(system: SystemMemory | null): MemoryTick[] {
  const max = sliderMaxMiB(system);
  const totalMib = system?.totalMib ?? max;
  let step = snapStepMiB;
  while ((max - minMemoryMiB) / step > maxTicks) {
    step *= 2;
  }
  const majorEvery = step === snapStepMiB ? 1024 : step * 2;

  const ticks: MemoryTick[] = [];
  for (let value = minMemoryMiB; value <= max; value += step) {
    ticks.push({
      valueMiB: value,
      pct: clamp((value / totalMib) * 100, 0, 100),
      major: value % majorEvery === 0,
    });
  }
  return ticks;
}
