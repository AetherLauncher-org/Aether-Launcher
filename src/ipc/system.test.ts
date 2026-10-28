import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getSystemMemory } from "./system";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

function pretendDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}

describe("getSystemMemory", () => {
  it("reports nothing in the browser preview without calling native code", async () => {
    await expect(getSystemMemory()).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns the machine's memory from the read-only command", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockResolvedValue({ totalMib: 16_284, availableMib: 2_355 });

    await expect(getSystemMemory()).resolves.toEqual({ totalMib: 16_284, availableMib: 2_355 });
    expect(invoke).toHaveBeenCalledWith("system_memory");
  });

  it("passes through a platform that reports no available memory", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockResolvedValue({ totalMib: 8192, availableMib: null });

    await expect(getSystemMemory()).resolves.toEqual({ totalMib: 8192, availableMib: null });
  });

  it("turns a failure or an unsupported platform into null instead of an error", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockRejectedValueOnce("boom");
    await expect(getSystemMemory()).resolves.toBeNull();

    vi.mocked(invoke).mockResolvedValueOnce(null);
    await expect(getSystemMemory()).resolves.toBeNull();
  });
});
