import { describe, expect, it } from "vitest";
import {
  clampMemoryMiB,
  defaultMemoryMiB,
  formatMemoryMiB,
  maxMemoryMiB,
  minMemoryMiB,
  parseStoredMemoryMiB,
} from "./memory";

describe("clampMemoryMiB", () => {
  it("keeps values inside the supported range", () => {
    expect(clampMemoryMiB(4096)).toBe(4096);
    expect(clampMemoryMiB(minMemoryMiB)).toBe(minMemoryMiB);
    expect(clampMemoryMiB(maxMemoryMiB)).toBe(maxMemoryMiB);
  });

  it("pulls values below the floor up to the floor", () => {
    expect(clampMemoryMiB(0)).toBe(minMemoryMiB);
    expect(clampMemoryMiB(-4096)).toBe(minMemoryMiB);
    expect(clampMemoryMiB(511)).toBe(minMemoryMiB);
  });

  it("pulls values above the ceiling down to the ceiling", () => {
    expect(clampMemoryMiB(maxMemoryMiB + 1)).toBe(maxMemoryMiB);
    expect(clampMemoryMiB(Number.MAX_SAFE_INTEGER)).toBe(maxMemoryMiB);
  });

  it("falls back to the default for missing or non-finite input", () => {
    expect(clampMemoryMiB(undefined)).toBe(defaultMemoryMiB);
    expect(clampMemoryMiB(null)).toBe(defaultMemoryMiB);
    expect(clampMemoryMiB(Number.NaN)).toBe(defaultMemoryMiB);
    expect(clampMemoryMiB(Number.POSITIVE_INFINITY)).toBe(defaultMemoryMiB);
  });

  it("rounds fractional values", () => {
    expect(clampMemoryMiB(2048.4)).toBe(2048);
    expect(clampMemoryMiB(2048.6)).toBe(2049);
  });
});

describe("formatMemoryMiB", () => {
  it("uses GiB from 1024 MiB upwards", () => {
    expect(formatMemoryMiB(1024)).toBe("1 GiB");
    expect(formatMemoryMiB(4096)).toBe("4 GiB");
    expect(formatMemoryMiB(6144)).toBe("6 GiB");
    expect(formatMemoryMiB(1536)).toBe("1.5 GiB");
  });

  it("stays in MiB below one GiB", () => {
    expect(formatMemoryMiB(512)).toBe("512 MiB");
    expect(formatMemoryMiB(768)).toBe("768 MiB");
  });

  it("clamps before formatting", () => {
    expect(formatMemoryMiB(1)).toBe("512 MiB");
    expect(formatMemoryMiB(maxMemoryMiB * 2)).toBe("128 GiB");
  });
});

describe("parseStoredMemoryMiB", () => {
  it("defaults when nothing is stored", () => {
    expect(parseStoredMemoryMiB(null)).toBe(defaultMemoryMiB);
  });

  it("reads a stored value", () => {
    expect(parseStoredMemoryMiB("8192")).toBe(8192);
  });

  it("clamps stored values that drifted out of range", () => {
    expect(parseStoredMemoryMiB("1")).toBe(minMemoryMiB);
    expect(parseStoredMemoryMiB("999999999")).toBe(maxMemoryMiB);
  });

  it("ignores garbage", () => {
    expect(parseStoredMemoryMiB("lots")).toBe(defaultMemoryMiB);
    expect(parseStoredMemoryMiB("")).toBe(defaultMemoryMiB);
  });
});
