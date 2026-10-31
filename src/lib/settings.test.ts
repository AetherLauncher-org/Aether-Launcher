import { describe, expect, it } from "vitest";
import {
  loadLastPlayedId,
  loadMemoryMiB,
  loadMemoryMode,
  memoryModeStorageKey,
  memoryStorageKey,
  saveLastPlayedId,
  saveMemoryMiB,
  saveMemoryMode,
} from "./settings";

describe("memory mode", () => {
  it("defaults to automatic on a fresh install", () => {
    expect(loadMemoryMode()).toBe("auto");
  });

  it("remembers the chosen mode", () => {
    saveMemoryMode("custom");
    expect(window.localStorage.getItem(memoryModeStorageKey)).toBe("custom");
    expect(loadMemoryMode()).toBe("custom");

    saveMemoryMode("auto");
    expect(loadMemoryMode()).toBe("auto");
  });

  it("keeps a value chosen before modes existed instead of silently dropping it", () => {
    window.localStorage.setItem(memoryStorageKey, "8192");

    expect(loadMemoryMode()).toBe("custom");
    expect(loadMemoryMiB()).toBe(8192);
  });

  it("ignores a corrupted mode", () => {
    window.localStorage.setItem(memoryModeStorageKey, "turbo");
    expect(loadMemoryMode()).toBe("auto");
  });
});

describe("memory value", () => {
  it("clamps what it saves and returns the stored value", () => {
    expect(saveMemoryMiB(1)).toBe(512);
    expect(saveMemoryMiB(999_999_999)).toBe(131_072);
    expect(saveMemoryMiB(6144)).toBe(6144);
    expect(loadMemoryMiB()).toBe(6144);
  });
});

describe("small remembered settings", () => {
  it("remembers the last played instance", () => {
    expect(loadLastPlayedId()).toBeNull();
    saveLastPlayedId("abc");
    expect(loadLastPlayedId()).toBe("abc");
  });
});
