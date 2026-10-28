import { describe, expect, it } from "vitest";
import {
  barGeometry,
  thumbForTrackPress,
  valueFromLeftDrag,
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

describe("valueFromLeftDrag", () => {
  // 1% 的轨道 = 163.84 MiB；步长 256
  it("allocates more for every step dragged left, and less for every step dragged right", () => {
    expect(valueFromLeftDrag(1024, 0, machine)).toBe(1024);
    expect(valueFromLeftDrag(1024, 5, machine)).toBe(1792);
    // 4096 少掉 25%（4096）就到了下限，不会变成 0 或负数
    expect(valueFromLeftDrag(4096, -25, machine)).toBe(512);
  });

  it("grows the blue share first: below full a leftward drag only raises the value, with the orange thumb still on the divider", () => {
    const value = valueFromLeftDrag(1024, 10, machine);
    const geometry = barGeometry(value, machine);

    expect(value).toBeGreaterThan(1024);
    expect(geometry.overMiB).toBe(0);
    expect(geometry.leftPct).toBeCloseTo(75);
  });

  it("carries on past full into the red area, where the orange thumb really moves left", () => {
    const value = valueFromLeftDrag(1024, 25, machine);
    const geometry = barGeometry(value, machine);

    expect(value).toBe(1024 + 4096);
    expect(geometry.overMiB).toBe(1024);
    expect(geometry.leftPct).toBeLessThan(75);
  });

  it("brings the orange thumb back to the divider first and then shrinks the blue share", () => {
    const start = 8192; // 比「满」多出 4096
    expect(barGeometry(valueFromLeftDrag(start, -25, machine), machine).overMiB).toBe(0);
    expect(valueFromLeftDrag(start, -25, machine)).toBe(4096);

    const further = valueFromLeftDrag(start, -30, machine);
    expect(further).toBeLessThan(4096);
    expect(barGeometry(further, machine).rightPct).toBeLessThan(100);
  });

  it("stays between the minimum and the installed memory", () => {
    expect(valueFromLeftDrag(1024, -100, machine)).toBe(512);
    expect(valueFromLeftDrag(1024, 500, machine)).toBe(16_384);
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
