import { beforeEach, describe, expect, it, vi } from "vitest";

const getSystemMemory = vi.fn();
vi.mock("../ipc", () => ({ getSystemMemory: () => getSystemMemory() }));

import { resolveLaunchMemoryMiB } from "./launchMemory";
import { saveMemoryMiB, saveMemoryMode } from "./settings";

beforeEach(() => {
  getSystemMemory.mockReset();
});

describe("resolveLaunchMemoryMiB", () => {
  it("reads the machine fresh at launch and recommends from what is free right now", async () => {
    getSystemMemory.mockResolvedValue({ totalMib: 16_384, availableMib: 10_240 });

    await expect(resolveLaunchMemoryMiB()).resolves.toBe(6656);
    expect(getSystemMemory).toHaveBeenCalledTimes(1);
  });

  it("uses the default when the machine's memory cannot be read", async () => {
    getSystemMemory.mockResolvedValue(null);

    await expect(resolveLaunchMemoryMiB()).resolves.toBe(4096);
  });

  it("uses the chosen value in custom mode without asking the machine", async () => {
    saveMemoryMode("custom");
    saveMemoryMiB(12_288);

    await expect(resolveLaunchMemoryMiB()).resolves.toBe(12_288);
    expect(getSystemMemory).not.toHaveBeenCalled();
  });
});
