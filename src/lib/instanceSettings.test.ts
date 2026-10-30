import { beforeEach, describe, expect, it, vi } from "vitest";
import { memoryModeStorageKey, memoryStorageKey } from "./settings";
import {
  clearInstanceSettings,
  instanceSettingsKey,
  loadInstanceSettings,
  noOverrides,
  resolveLaunchSettings,
  saveInstanceSettings,
} from "./instanceSettings";

vi.mock("../ipc", () => ({ getSystemMemory: vi.fn(async () => ({ totalMib: 16_384, availableMib: 8192 })) }));

describe("instanceSettings", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("follows the global setting until something is set", () => {
    expect(loadInstanceSettings("i1")).toEqual(noOverrides);
  });

  it("keeps a choice per instance", () => {
    saveInstanceSettings("i1", { memoryMiB: 4096 });
    saveInstanceSettings("i2", { memoryMiB: 8192 });

    expect(loadInstanceSettings("i1")).toEqual({ memoryMiB: 4096 });
    expect(loadInstanceSettings("i2")).toEqual({ memoryMiB: 8192 });
    expect(loadInstanceSettings("i3")).toEqual(noOverrides);
  });

  it("clamps the memory it saves", () => {
    const saved = saveInstanceSettings("i1", { memoryMiB: 1 });
    expect(saved.memoryMiB).toBeGreaterThan(1);
    expect(loadInstanceSettings("i1").memoryMiB).toBe(saved.memoryMiB);
  });

  it("removes the record when the instance follows the global setting again", () => {
    saveInstanceSettings("i1", { memoryMiB: 4096 });
    saveInstanceSettings("i1", noOverrides);
    expect(window.localStorage.getItem(instanceSettingsKey("i1"))).toBeNull();
  });

  it("clears an instance's settings", () => {
    saveInstanceSettings("i1", { memoryMiB: 4096 });
    clearInstanceSettings("i1");
    expect(loadInstanceSettings("i1")).toEqual(noOverrides);
  });

  it("ignores a record it cannot understand, and a Java field from nowhere", () => {
    for (const junk of ["", "not json", "null", "[]", '"x"', '{"memoryMiB":"lots"}']) {
      window.localStorage.setItem(instanceSettingsKey("i1"), junk);
      expect(loadInstanceSettings("i1")).toEqual(noOverrides);
    }
    window.localStorage.setItem(instanceSettingsKey("i1"), '{"memoryMiB":2048,"javaKey":"c:/evil/java.exe"}');
    expect(loadInstanceSettings("i1")).toEqual({ memoryMiB: 2048 });
  });

  it("works when storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(loadInstanceSettings("i1")).toEqual(noOverrides);
    expect(() => saveInstanceSettings("i1", { memoryMiB: 4096 })).not.toThrow();
    expect(() => clearInstanceSettings("i1")).not.toThrow();
  });

  it("launches with the instance's own memory first, then the global one", async () => {
    window.localStorage.setItem(memoryModeStorageKey, "custom");
    window.localStorage.setItem(memoryStorageKey, "3072");

    expect(await resolveLaunchSettings("i1")).toEqual({ memoryMiB: 3072 });

    saveInstanceSettings("i1", { memoryMiB: 6144 });
    expect(await resolveLaunchSettings("i1")).toEqual({ memoryMiB: 6144 });

    // Another instance is unaffected.
    expect(await resolveLaunchSettings("i2")).toEqual({ memoryMiB: 3072 });
  });
});
