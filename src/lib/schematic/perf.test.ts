import { describe, expect, it } from "vitest";
import {
  FrameBudget,
  MAX_RENDER_SCALE,
  MIN_RENDER_SCALE,
  MIN_VIEW_FRACTION,
  snapRefreshRate,
} from "./perf";

/** 以固定间隔和固定的工作量喂 n 帧。 */
function feed(budget: FrameBudget, frames: number, interval: number, work: number) {
  for (let i = 0; i < frames; i += 1) {
    budget.frame(interval, work);
  }
}

describe("snapRefreshRate", () => {
  it("recognises the refresh rates monitors really have", () => {
    expect(snapRefreshRate(1000 / 60)).toBe(60);
    expect(snapRefreshRate(1000 / 144)).toBe(144);
    expect(snapRefreshRate(1000 / 240)).toBe(240);
    expect(snapRefreshRate(1000 / 59.94)).toBe(60);
    expect(snapRefreshRate(1000 / 143.9)).toBe(144);
    expect(snapRefreshRate(1000 / 165)).toBe(165);
  });

  it("keeps an odd rate as it is and defaults to 60 for nonsense", () => {
    expect(snapRefreshRate(1000 / 110)).toBe(110);
    expect(snapRefreshRate(0)).toBe(60);
    expect(snapRefreshRate(-4)).toBe(60);
    expect(snapRefreshRate(Number.NaN)).toBe(60);
  });
});

describe("FrameBudget", () => {
  it("starts with a 60 Hz budget and full quality", () => {
    const budget = new FrameBudget();
    expect(budget.budgetMs).toBeCloseTo(16.67, 1);
    expect(budget.quality).toEqual({ renderScale: MAX_RENDER_SCALE, viewFraction: 1 });
  });

  it("follows the display: a 144 Hz monitor gets a 6.9 ms budget, a 240 Hz one 4.2 ms", () => {
    const fast = new FrameBudget();
    feed(fast, 120, 1000 / 144, 2);
    expect(fast.stats().refreshHz).toBe(144);
    expect(fast.budgetMs).toBeCloseTo(6.94, 1);

    const faster = new FrameBudget();
    feed(faster, 120, 1000 / 240, 1);
    expect(faster.stats().refreshHz).toBe(240);
    expect(faster.budgetMs).toBeCloseTo(4.17, 1);
  });

  it("scales the meshing slice with the budget and never exceeds 3 ms", () => {
    const sixty = new FrameBudget();
    feed(sixty, 120, 1000 / 60, 2);
    expect(sixty.meshBudgetMs).toBeCloseTo(2, 1);

    const fast = new FrameBudget();
    feed(fast, 120, 1000 / 240, 0.5);
    expect(fast.meshBudgetMs).toBeCloseTo(0.5, 1);

    const slow = new FrameBudget();
    feed(slow, 120, 1000 / 30, 1);
    expect(slow.meshBudgetMs).toBeLessThanOrEqual(3);
  });

  it("gives no time to meshing when the last frame was already busy", () => {
    const budget = new FrameBudget();
    feed(budget, 60, 1000 / 60, 12);
    expect(budget.meshBudgetMs).toBe(0);
  });

  it("does not mistake a slow machine for a slow display", () => {
    const budget = new FrameBudget();
    // Always 40 ms per frame (25 fps): the display is still at least 60 Hz.
    feed(budget, 300, 40, 5);

    expect(budget.stats().refreshHz).toBe(60);
    expect(budget.budgetMs).toBeCloseTo(16.67, 1);
    expect(budget.stats().missedFrames).toBeGreaterThan(200);
    expect(budget.quality.renderScale).toBeLessThan(1);
  });

  it("holds full quality while frames keep to the budget", () => {
    const budget = new FrameBudget();
    feed(budget, 600, 1000 / 60, 5);
    expect(budget.quality).toEqual({ renderScale: MAX_RENDER_SCALE, viewFraction: 1 });
    expect(budget.stats().missedFrames).toBe(0);
  });

  it("lowers the render scale first when frames are missed, one step at a time", () => {
    const budget = new FrameBudget();
    feed(budget, 40, 1000 / 60, 5);

    feed(budget, 25, 40, 5); // far longer than a frame: the GPU cannot keep up
    const first = budget.quality;
    expect(first.renderScale).toBeCloseTo(0.9, 5);
    expect(first.viewFraction).toBe(1);

    feed(budget, 200, 40, 5);
    expect(budget.quality.renderScale).toBe(MIN_RENDER_SCALE);
  });

  it("shortens the view distance only after the resolution is as low as it goes", () => {
    const budget = new FrameBudget();
    feed(budget, 40, 1000 / 60, 5);
    feed(budget, 2000, 40, 5);

    expect(budget.quality.renderScale).toBe(MIN_RENDER_SCALE);
    expect(budget.quality.viewFraction).toBe(MIN_VIEW_FRACTION);
    // And it never goes below the floors.
    feed(budget, 2000, 40, 5);
    expect(budget.quality).toEqual({ renderScale: MIN_RENDER_SCALE, viewFraction: MIN_VIEW_FRACTION });
  });

  it("degrades when the CPU side alone eats the budget, even before frames are missed", () => {
    const budget = new FrameBudget();
    feed(budget, 40, 1000 / 60, 5);

    feed(budget, 25, 1000 / 60, 15); // 15 ms of work in a 16.7 ms frame

    expect(budget.quality.renderScale).toBeLessThan(1);
  });

  it("ignores a single long frame", () => {
    const budget = new FrameBudget();
    feed(budget, 60, 1000 / 60, 5);

    budget.frame(120, 5);
    feed(budget, 60, 1000 / 60, 5);

    expect(budget.quality.renderScale).toBe(MAX_RENDER_SCALE);
    expect(budget.stats().missedFrames).toBe(1);
  });

  it("ignores the huge gap after the tab was hidden and the first frames while warming up", () => {
    const budget = new FrameBudget();
    // Slow frames during warm-up do not count.
    feed(budget, 20, 60, 5);
    expect(budget.stats().missedFrames).toBe(0);
    feed(budget, 60, 1000 / 60, 5);

    expect(budget.frame(30_000, 5)).toBe(false);
    expect(budget.stats().missedFrames).toBe(0);
    expect(budget.quality.renderScale).toBe(MAX_RENDER_SCALE);
  });

  it("brings quality back slowly once frames are comfortable again", () => {
    const budget = new FrameBudget();
    feed(budget, 40, 1000 / 60, 5);
    feed(budget, 100, 40, 5);
    // Let the slow frames leave the measuring window before taking the reading.
    feed(budget, 60, 1000 / 60, 2);
    const lowered = budget.quality.renderScale;
    expect(lowered).toBeLessThan(1);

    feed(budget, 100, 1000 / 60, 2); // comfortable, but not for long enough to trust yet
    expect(budget.quality.renderScale).toBe(lowered);

    feed(budget, 3000, 1000 / 60, 2);
    expect(budget.quality).toEqual({ renderScale: MAX_RENDER_SCALE, viewFraction: 1 });
  });

  it("counts missed frames against the display's own budget", () => {
    const budget = new FrameBudget();
    feed(budget, 120, 1000 / 144, 2);

    // 12 ms is fine at 60 Hz and a miss at 144 Hz.
    expect(budget.frame(12, 2)).toBe(true);
    expect(budget.stats().missedFrames).toBe(1);
  });

  it("reports the numbers the F3 panel shows", () => {
    const budget = new FrameBudget();
    feed(budget, 100, 1000 / 60, 4);

    const stats = budget.stats();

    expect(stats.refreshHz).toBe(60);
    expect(stats.totalFrames).toBe(100);
    expect(stats.intervalP95).toBeCloseTo(16.67, 1);
    expect(stats.workP95).toBe(4);
  });
});
