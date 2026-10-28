import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listMinecraftVersions } from "./catalog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("listMinecraftVersions", () => {
  it("does not try to fetch versions in the browser preview", async () => {
    await expect(listMinecraftVersions()).rejects.toThrow("需要桌面版");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("calls the fixed read-only catalog IPC command", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    const catalog = {
      latest: { release: "1.21.1", snapshot: "24w14a" },
      versions: [
        {
          id: "1.21.1",
          type: "release",
          sha1: "0123456789abcdef0123456789abcdef01234567",
          url: "https://piston-meta.mojang.com/v1/packages/example/1.21.1.json",
          releaseTime: "2024-08-08T00:00:00+00:00",
        },
      ],
    };
    vi.mocked(invoke).mockResolvedValue(catalog);
    await expect(listMinecraftVersions()).resolves.toEqual(catalog);
    expect(invoke).toHaveBeenCalledWith("list_minecraft_versions");
  });
});
