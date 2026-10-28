import { describe, expect, it } from "vitest";
import {
  barGeometry,
  thumbForTrackPress,
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
    expect(geometry.leftPct).toBeNull();
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
    expect(valueFromRightThumb(80, machine) % 256).toBe(0);
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

describe("thumbForTrackPress", () => {
  it("moves the right thumb when the press is in the free area or nothing is known", () => {
    expect(thumbForTrackPress(90, barGeometry(1024, machine))).toBe("right");
    expect(thumbForTrackPress(10, barGeometry(1024, null))).toBe("right");
  });

  it("ignores a press in the red area until the allocation is full, then moves the second thumb", () => {
    expect(thumbForTrackPress(40, barGeometry(1024, machine))).toBeNull();
    expect(thumbForTrackPress(40, barGeometry(4096, machine))).toBe("left");
  });
});
