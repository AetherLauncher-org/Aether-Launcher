import { describe, expect, it } from "vitest";
import {
  describeMemorySetting,
  effectiveMemoryMiB,
  exceedsAvailable,
  fallbackSliderMaxMiB,
  formatMiB,
  isMemoryMode,
  maxMemoryMiB,
  recommendedMemoryMiB,
  sliderMaxMiB,
  usedMiB,
} from "./memory";

const GiB = 1024;
const machine = (totalGiB: number, availableGiB: number | null) => ({
  totalMib: totalGiB * GiB,
  availableMib: availableGiB === null ? null : availableGiB * GiB,
});

describe("sliderMaxMiB", () => {
  it("is the installed memory rounded down to the slider step", () => {
    expect(sliderMaxMiB({ totalMib: 16_300, availableMib: 8000 })).toBe(16_128);
    expect(sliderMaxMiB(machine(32, 20))).toBe(32 * GiB);
  });

  it("never exceeds what the backend accepts", () => {
    expect(sliderMaxMiB(machine(512, 400))).toBe(maxMemoryMiB);
  });

  it("estimates 16 GiB when the machine's memory is unknown", () => {
    expect(sliderMaxMiB(null)).toBe(fallbackSliderMaxMiB);
  });

  it("keeps a draggable range on a tiny machine", () => {
    expect(sliderMaxMiB({ totalMib: 300, availableMib: 100 })).toBeGreaterThan(512);
  });
});

describe("usedMiB and exceedsAvailable", () => {
  it("is total minus available, and unknown without a reading", () => {
    expect(usedMiB(machine(16, 2.5))).toBe(13.5 * GiB);
    expect(usedMiB(machine(16, null))).toBeNull();
    expect(usedMiB(null)).toBeNull();
  });

  it("flags an allocation larger than the available memory, and does not guess otherwise", () => {
    expect(exceedsAvailable(9 * GiB, machine(16, 2.3))).toBe(true);
    expect(exceedsAvailable(2 * GiB, machine(16, 2.3))).toBe(false);
    expect(exceedsAvailable(2 * GiB, machine(16, null))).toBeNull();
    expect(exceedsAvailable(2 * GiB, null)).toBeNull();
  });
});

describe("recommendedMemoryMiB", () => {
  it("takes 65% of the available memory, rounded down to the step", () => {
    // 10 GiB available → 6.5 GiB → 6.5 GiB is a multiple of 256
    expect(recommendedMemoryMiB(machine(16, 10))).toBe(6656);
  });

  it("stops at 8 GiB however much is free", () => {
    expect(recommendedMemoryMiB(machine(64, 50))).toBe(8192);
  });

  it("does not go below 2 GiB when the machine is busy", () => {
    expect(recommendedMemoryMiB(machine(16, 2.3))).toBe(2048);
  });

  it("never exceeds 75% of the installed memory, which only matters to the 2 GiB floor on tiny machines", () => {
    expect(recommendedMemoryMiB(machine(4, 4))).toBe(2560);
    expect(recommendedMemoryMiB(machine(3, 3))).toBe(2048);
    // 2 GiB machine: the floor yields to the cap (1.5 GiB)
    expect(recommendedMemoryMiB(machine(2, 2))).toBe(1536);
  });

  it("is a multiple of the step and inside the accepted range", () => {
    for (const [total, free] of [[8, 5], [12, 1], [16, 12], [128, 100]] as const) {
      const value = recommendedMemoryMiB(machine(total, free));
      expect(value % 256).toBe(0);
      expect(value).toBeGreaterThanOrEqual(512);
      expect(value).toBeLessThanOrEqual(maxMemoryMiB);
    }
  });

  it("assumes a modest share is free when only the total is known, and falls back to the default when nothing is", () => {
    expect(recommendedMemoryMiB(machine(16, null))).toBeGreaterThan(2048);
    expect(recommendedMemoryMiB(null)).toBe(4096);
  });
});

describe("effectiveMemoryMiB and describeMemorySetting", () => {
  it("uses the recommendation in auto mode and the clamped chosen value in custom mode", () => {
    expect(effectiveMemoryMiB("auto", 12_288, machine(16, 10))).toBe(6656);
    expect(effectiveMemoryMiB("custom", 12_288, machine(16, 10))).toBe(12_288);
    expect(effectiveMemoryMiB("custom", 1, null)).toBe(512);
  });

  it("describes the setting without inventing a number for auto", () => {
    expect(describeMemorySetting("auto", 8192)).toBe("自动");
    expect(describeMemorySetting("custom", 8192)).toBe("8 GiB");
  });

  it("recognises only the two modes", () => {
    expect(isMemoryMode("auto")).toBe(true);
    expect(isMemoryMode("custom")).toBe(true);
    expect(isMemoryMode("manual")).toBe(false);
    expect(isMemoryMode(null)).toBe(false);
  });
});

describe("formatMiB", () => {
  it("shows small values as they are instead of lifting them to the 512 MiB floor", () => {
    expect(formatMiB(300)).toBe("300 MiB");
    expect(formatMiB(0)).toBe("0 MiB");
    expect(formatMiB(-5)).toBe("0 MiB");
  });

  it("switches to GiB from 1024 MiB", () => {
    expect(formatMiB(1024)).toBe("1 GiB");
    expect(formatMiB(13.6 * GiB)).toBe("13.6 GiB");
    expect(formatMiB(9 * GiB)).toBe("9 GiB");
  });
});
