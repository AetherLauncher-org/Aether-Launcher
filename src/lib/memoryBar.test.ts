import { describe, expect, it } from "vitest";
import {
  barGeometry,
  memoryTicks,
  snapStepMiB,
  thumbForTrackPress,
  valueFromDrag,
  valueFromLeftThumb,
  valueFromRightThumb,
} from "./memoryBar";

// 16 GiB 的机器，已用 12 GiB，可用 4 GiB
const machine = { totalMib: 16_384, availableMib: 4096 };

describe("barGeometry", () => {
  it("lays the system's use out from the left and the game's share on top of the free area", () => {
    const geometry = barGeometry(1024, machine);

    expect(geometry.usedPct).toBeCloseTo(75);
    expect(geometry.fullMib).toBe(4096);
    expect(geometry.rightPct).toBeCloseTo(75 + 6.25);
    // 橙色手柄一直在：没越界时就停在分界线上
    expect(geometry.leftPct).toBeCloseTo(75);
    expect(geometry.overMiB).toBe(0);
  });

  it("puts the right thumb at the far end when all the free memory is allocated", () => {
    const geometry = barGeometry(4096, machine);

    expect(geometry.rightPct).toBeCloseTo(100);
    // 刚好「满」：第二个手柄出现在分界线上
    expect(geometry.leftPct).toBeCloseTo(75);
    expect(geometry.overMiB).toBe(0);
  });

  it("moves the second thumb into the red area as the allocation passes the free memory", () => {
    const geometry = barGeometry(4096 + 4096, machine);

    expect(geometry.rightPct).toBeCloseTo(100);
    expect(geometry.overMiB).toBe(4096);
    expect(geometry.leftPct).toBeCloseTo(75 - 25);
  });

  it("reaches the left end of the track when everything is allocated", () => {
    const geometry = barGeometry(16_384, machine);

    expect(geometry.leftPct).toBeCloseTo(0);
    expect(geometry.overMiB).toBe(12_288);
  });

  it("clamps a value outside the range instead of drawing off the track", () => {
    expect(barGeometry(1, machine).valueMiB).toBe(512);
    expect(barGeometry(10_000_000, machine).valueMiB).toBe(16_384);
  });

  it("has no red area and no second thumb when the used memory is unknown", () => {
    for (const system of [null, { totalMib: 8192, availableMib: null }]) {
      const geometry = barGeometry(4096, system);
      expect(geometry.usedMib).toBeNull();
      expect(geometry.usedPct).toBe(0);
      expect(geometry.leftPct).toBeNull();
      expect(geometry.overMiB).toBe(0);
    }
    expect(barGeometry(4096, { totalMib: 8192, availableMib: null }).rightPct).toBeCloseTo(50);
  });
});

describe("dragging the right thumb", () => {
  it("only picks values inside the free area, snapped to the step", () => {
    // 分界线在 75%，右端 100% 对应 4096 MiB 可用
    expect(valueFromRightThumb(75 + 6.25, machine)).toBe(1024);
    expect(valueFromRightThumb(100, machine)).toBe(4096);
    expect(valueFromRightThumb(80, machine) % 32).toBe(0);
  });

  it("cannot be dragged into the red area or below the minimum", () => {
    expect(valueFromRightThumb(10, machine)).toBe(512);
    expect(valueFromRightThumb(75, machine)).toBe(512);
  });

  it("is a plain slider over the whole track when the used memory is unknown", () => {
    expect(valueFromRightThumb(50, { totalMib: 8192, availableMib: null })).toBe(4096);
    expect(valueFromRightThumb(100, null)).toBe(16_384);
  });
});

describe("dragging the second thumb", () => {
  it("starts from full and adds the red area it passes over", () => {
    expect(valueFromLeftThumb(75, machine)).toBe(4096);
    expect(valueFromLeftThumb(50, machine)).toBe(4096 + 4096);
    expect(valueFromLeftThumb(0, machine)).toBe(16_384);
  });

  it("never goes below full or past the installed memory", () => {
    expect(valueFromLeftThumb(90, machine)).toBe(4096);
    expect(valueFromLeftThumb(-20, machine)).toBe(16_384);
  });

  it("agrees with the geometry it came from", () => {
    for (const value of [4096, 6144, 10_240, 16_384]) {
      const geometry = barGeometry(value, machine);
      expect(valueFromLeftThumb(geometry.leftPct ?? 0, machine)).toBe(value);
    }
  });
});

describe("valueFromDrag", () => {
  // 1% 的轨道 = 163.84 MiB；指针取整粒度 32 MiB，所以手柄看上去是连续的
  it("allocates more for a positive move and less for a negative one", () => {
    expect(valueFromDrag(1024, 0, machine)).toBe(1024);
    // 1024 + 819.2 = 1843.2，吸附到最近的 512 的倍数
    expect(valueFromDrag(1024, 5, machine)).toBe(2048);
    expect(valueFromDrag(1024, 5, machine, true)).toBe(1856);
    // 4096 少掉 25%（4096）就到了下限，不会变成 0 或负数
    expect(valueFromDrag(4096, -25, machine)).toBe(512);
  });

  it("snaps to multiples of 512 MiB by default", () => {
    for (const pct of [0.7, 1.3, 2.9, 6.1, 11.4, 17.8]) {
      expect(valueFromDrag(1024, pct, machine) % snapStepMiB).toBe(0);
      expect(valueFromRightThumb(75 + pct, machine) % snapStepMiB).toBe(0);
    }
    // 小于半格的移动不会让值动
    expect(valueFromDrag(1024, 0.5, machine)).toBe(1024);
    // 从不在网格上的值出发，第一次移动就落到网格上
    expect(valueFromDrag(1100, 0.1, machine)).toBe(1024);
  });

  it("still reaches the minimum and the end of the track when they are not on the grid", () => {
    const odd = { totalMib: 8448, availableMib: 2304 }; // 8448 = 16.5 × 512
    expect(valueFromDrag(1024, 500, odd)).toBe(8448);
    expect(valueFromDrag(1024, -500, odd)).toBe(512);
  });

  it("snaps the orange thumb's total allocation, not just the part past full", () => {
    // 从「满」(4096) 开始加红区；4096 + 700 = 4796 → 4608（吸附），细粒度 4800
    const pct = 75 - (700 / 16_384) * 100;
    expect(valueFromLeftThumb(pct, machine)).toBe(4608);
    expect(valueFromLeftThumb(pct, machine, true)).toBe(4800);
  });

  it("moves in steps fine enough to look continuous when Shift turns snapping off, not in the 256 MiB steps of the keyboard", () => {
    const values = [0.1, 0.2, 0.3, 0.4, 0.5].map((pct) => valueFromDrag(1024, pct, machine, true));

    for (const value of values) {
      expect(value % 32).toBe(0);
    }
    expect(new Set(values).size).toBeGreaterThan(1);
    expect(valueFromDrag(1024, 0.5, machine, true)).toBeLessThan(1024 + 256);
  });

  it("grows the blue share first: below full an increasing drag only raises the value, with the orange thumb still on the divider", () => {
    const value = valueFromDrag(1024, 10, machine);
    const geometry = barGeometry(value, machine);

    expect(value).toBeGreaterThan(1024);
    expect(geometry.overMiB).toBe(0);
    expect(geometry.leftPct).toBeCloseTo(75);
  });

  it("carries on past full into the red area, where the orange thumb really moves left", () => {
    const value = valueFromDrag(1024, 25, machine);
    const geometry = barGeometry(value, machine);

    expect(value).toBe(1024 + 4096);
    expect(geometry.overMiB).toBe(1024);
    expect(geometry.leftPct).toBeLessThan(75);
  });

  it("brings the orange thumb back to the divider first and then shrinks the blue share", () => {
    const start = 8192; // 比「满」多出 4096
    expect(barGeometry(valueFromDrag(start, -25, machine), machine).overMiB).toBe(0);
    expect(valueFromDrag(start, -25, machine)).toBe(4096);

    const further = valueFromDrag(start, -30, machine);
    expect(further).toBeLessThan(4096);
    expect(barGeometry(further, machine).rightPct).toBeLessThan(100);
  });

  it("stays between the minimum and the installed memory", () => {
    expect(valueFromDrag(1024, -100, machine)).toBe(512);
    expect(valueFromDrag(1024, 500, machine)).toBe(16_384);
  });
});

describe("memoryTicks", () => {
  it("puts a tick on every 512 MiB the thumbs can snap to, at the position the thumb would have", () => {
    const ticks = memoryTicks(machine);

    expect(ticks[0].valueMiB).toBe(512);
    expect(ticks.every((tick) => tick.valueMiB % 512 === 0)).toBe(true);
    for (const tick of ticks) {
      const geometry = barGeometry(tick.valueMiB, machine);
      const where = tick.valueMiB > geometry.fullMib ? geometry.leftPct : geometry.rightPct;
      expect(tick.pct).toBeCloseTo(where ?? -1, 5);
    }
  });

  it("continues into the red area past the divider", () => {
    const beyond = memoryTicks(machine).filter((tick) => tick.valueMiB > 4096);

    expect(beyond.length).toBeGreaterThan(0);
    expect(beyond.every((tick) => tick.pct < 75)).toBe(true);
  });

  it("marks whole GiB as major", () => {
    const ticks = memoryTicks(machine);

    expect(ticks.find((tick) => tick.valueMiB === 1024)?.major).toBe(true);
    expect(ticks.find((tick) => tick.valueMiB === 1536)?.major).toBe(false);
  });

  it("thins out on a big machine instead of drawing hundreds of ticks", () => {
    const ticks = memoryTicks({ totalMib: 131_072, availableMib: 65_536 });

    expect(ticks.length).toBeLessThanOrEqual(33);
    expect(ticks[1].valueMiB - ticks[0].valueMiB).toBeGreaterThan(512);
  });

  it("spans the whole track when the used memory is unknown", () => {
    const ticks = memoryTicks({ totalMib: 8192, availableMib: null });

    expect(ticks[0].pct).toBeCloseTo((512 / 8192) * 100);
    expect(ticks[ticks.length - 1].pct).toBeCloseTo(100);
  });
});

describe("thumbForTrackPress", () => {
  it("moves the blue thumb when the press is right of the divider or nothing is known", () => {
    expect(thumbForTrackPress(90, barGeometry(1024, machine))).toBe("right");
    expect(thumbForTrackPress(10, barGeometry(1024, null))).toBe("right");
  });

  it("puts the orange thumb where the press is in the red area, whatever the allocation", () => {
    expect(thumbForTrackPress(40, barGeometry(1024, machine))).toBe("left");
    expect(thumbForTrackPress(40, barGeometry(4096, machine))).toBe("left");
  });
});
