/**
 * 3D 预览的帧预算。
 *
 * 目标不是固定 60 帧，而是「跟着显示器走」：`requestAnimationFrame` 的间隔就是显示器的
 * 刷新周期，60、120、144、240 Hz 都一样。这里先量出刷新率，再把「每帧能花多少毫秒」
 * 算出来，其余的事（渲染分辨率、可见距离、建网格的时间片）都按这个预算调。
 *
 * 能保证的是：每帧里启动器自己做的事有上限，慢了会先降画质、再降距离，而不是等掉帧
 * 以后才发现。不能保证的是：操作系统调度、垃圾回收、别的程序抢 CPU/GPU 造成的偶发卡顿。
 * 掉帧数会如实显示在 F3 面板里。
 */

/** 常见刷新率；量出来的值在 6% 以内就吸附过去，避免 143.9 之类的零头。 */
const COMMON_RATES = [30, 48, 50, 60, 72, 75, 90, 100, 120, 144, 165, 180, 200, 240, 360] as const;

/** 不管量出来多慢，都按至少这个刷新率来算预算。 */
const MIN_REFRESH_HZ = 60;

const WINDOW = 90;
const WARM_UP_FRAMES = 30;
/** 间隔超过预算的这么多倍，就算掉了一帧。 */
const MISSED_FACTOR = 1.5;
/** 连续这么多帧都吃紧才降一档，避免一次卡顿就把画质打下去。 */
const DEGRADE_AFTER = 20;
/** 连续这么多帧都很宽裕才升一档。慢升快降，免得来回抖。 */
const IMPROVE_AFTER = 180;

export const MIN_RENDER_SCALE = 0.5;
export const MAX_RENDER_SCALE = 1;
const SCALE_STEP = 0.1;
export const MIN_VIEW_FRACTION = 0.25;
const VIEW_STEP = 0.25;

/** 把量到的帧间隔（毫秒）换成显示器刷新率（Hz）。 */
export function snapRefreshRate(intervalMs: number): number {
  if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
    return 60;
  }
  const measured = 1000 / intervalMs;
  const nearest = COMMON_RATES.reduce((best, rate) =>
    Math.abs(rate - measured) < Math.abs(best - measured) ? rate : best,
  );
  return Math.abs(nearest - measured) / nearest <= 0.06 ? nearest : Math.round(measured);
}

function percentile(values: readonly number[], fraction: number): number {
  if (values.length === 0) {
    return 0;
  }
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))];
}

export type QualityLevel = {
  /** 渲染分辨率相对窗口的比例，0.5–1 */
  renderScale: number;
  /** 可见距离占全部范围的比例，0.25–1 */
  viewFraction: number;
};

export type FrameStats = {
  refreshHz: number;
  budgetMs: number;
  /** 最近一批帧的间隔（毫秒）的 95 分位 */
  intervalP95: number;
  /** 最近一批帧里启动器自己花的时间的 95 分位 */
  workP95: number;
  /** 掉了的帧数（间隔超过预算的 1.5 倍），自开始起累计 */
  missedFrames: number;
  totalFrames: number;
  quality: QualityLevel;
};

/**
 * 每帧调用 `frame`，报告这一帧距上一帧过了多久、启动器自己花了多久。读出 `quality`
 * 决定下一帧的分辨率和距离，读出 `meshBudgetMs` 决定这一帧还能不能建网格。
 */
export class FrameBudget {
  private intervals: number[] = [];
  private work: number[] = [];
  private refreshHz = 60;
  private frames = 0;
  private missed = 0;
  private strained = 0;
  private relaxed = 0;
  private level: QualityLevel = { renderScale: MAX_RENDER_SCALE, viewFraction: 1 };
  private lastWorkMs = 0;

  /** 每帧可用的毫秒数，随显示器刷新率变化。 */
  get budgetMs(): number {
    return 1000 / this.refreshHz;
  }

  get quality(): QualityLevel {
    return this.level;
  }

  /** 前几帧不算：页面刚打开、纹理刚上传时的帧不代表稳态。 */
  private get warm(): boolean {
    return this.frames > WARM_UP_FRAMES;
  }

  /**
   * 记录一帧。`intervalMs` 是和上一帧的时间差，`workMs` 是这一帧里 JS 花的时间。
   * 返回这一帧是不是掉帧。
   */
  frame(intervalMs: number, workMs: number): boolean {
    this.frames += 1;
    this.lastWorkMs = workMs;
    // 页面被切走再回来会有一个巨大的间隔，它不代表刷新率，也不该触发降级。
    if (!Number.isFinite(intervalMs) || intervalMs <= 0 || intervalMs > 500) {
      return false;
    }
    this.intervals.push(intervalMs);
    this.work.push(workMs);
    if (this.intervals.length > WINDOW) {
      this.intervals.shift();
      this.work.shift();
    }
    // 刷新率取最近一批间隔里偏快的那一端（第 10 百分位）：帧不可能比显示器刷新得更快，
    // 慢帧只说明我们跟不上，不说明显示器慢。再设 60 Hz 的下限，否则一台一直只有 25 帧的
    // 机器会被当成 25 Hz 的显示器，永远不会降画质。
    if (this.intervals.length >= WARM_UP_FRAMES && this.frames % 15 === 0) {
      this.refreshHz = Math.max(MIN_REFRESH_HZ, snapRefreshRate(percentile(this.intervals, 0.1)));
    }

    const missed = this.warm && intervalMs > this.budgetMs * MISSED_FACTOR;
    if (missed) {
      this.missed += 1;
    }
    if (this.warm) {
      this.adapt();
    }
    return missed;
  }

  private adapt(): void {
    const recent = this.intervals.slice(-30);
    const recentWork = this.work.slice(-30);
    const missRate =
      recent.filter((interval) => interval > this.budgetMs * MISSED_FACTOR).length / recent.length;
    const tight = missRate > 0.1 || percentile(recentWork, 0.95) > this.budgetMs * 0.8;
    const roomy = missRate === 0 && percentile(recentWork, 0.95) < this.budgetMs * 0.4;

    this.strained = tight ? this.strained + 1 : 0;
    this.relaxed = roomy ? this.relaxed + 1 : 0;

    if (this.strained >= DEGRADE_AFTER) {
      this.strained = 0;
      this.step(-1);
    } else if (this.relaxed >= IMPROVE_AFTER) {
      this.relaxed = 0;
      this.step(1);
    }
  }

  /** 降档先降分辨率（GPU 压力最直接），降到底再缩可见距离；升档反过来。 */
  private step(direction: 1 | -1): void {
    const { renderScale, viewFraction } = this.level;
    if (direction < 0) {
      if (renderScale > MIN_RENDER_SCALE + 1e-9) {
        this.level = { renderScale: round(renderScale - SCALE_STEP), viewFraction };
      } else if (viewFraction > MIN_VIEW_FRACTION + 1e-9) {
        this.level = { renderScale, viewFraction: round(viewFraction - VIEW_STEP) };
      }
    } else if (viewFraction < 1 - 1e-9) {
      this.level = { renderScale, viewFraction: round(viewFraction + VIEW_STEP) };
    } else if (renderScale < MAX_RENDER_SCALE - 1e-9) {
      this.level = { renderScale: round(renderScale + SCALE_STEP), viewFraction };
    }
  }

  /**
   * 这一帧还能拿多少毫秒来建网格。上一帧已经吃紧就一毫秒都不给；宽裕时给预算的约
   * 12%，最多 3 ms——单核机器上建网格和渲染抢同一个核，必须小片小片地做。
   */
  get meshBudgetMs(): number {
    if (this.lastWorkMs > this.budgetMs * 0.6) {
      return 0;
    }
    return Math.min(3, this.budgetMs * 0.12);
  }

  stats(): FrameStats {
    return {
      refreshHz: this.refreshHz,
      budgetMs: this.budgetMs,
      intervalP95: percentile(this.intervals, 0.95),
      workP95: percentile(this.work, 0.95),
      missedFrames: this.missed,
      totalFrames: this.frames,
      quality: this.level,
    };
  }
}

function round(value: number): number {
  return Math.round(value * 100) / 100;
}
