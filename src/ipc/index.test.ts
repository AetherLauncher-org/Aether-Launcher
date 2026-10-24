import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { getApplicationInfo, isTauri, listJavaRuntimes } from "./index";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.mocked(invoke).mockReset();
});

describe("isTauri", () => {
  it("is false in a plain browser", () => {
    expect(isTauri()).toBe(false);
  });

  it("is true once the Tauri runtime is injected", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    expect(isTauri()).toBe(true);
  });
});

describe("getApplicationInfo", () => {
  it("calls the application_info command and returns its result", async () => {
    const info = {
      name: "Aether Launcher",
      version: "0.1.0",
      coreStatus: "foundation",
    };
    vi.mocked(invoke).mockResolvedValue(info);

    await expect(getApplicationInfo()).resolves.toEqual(info);
    expect(invoke).toHaveBeenCalledWith("application_info");
  });
});

describe("listJavaRuntimes", () => {
  it("returns typed Java runtime records from the IPC command", async () => {
    const runtimes = [
      {
        executable: "C:\\Program Files\\Java\\bin\\java.exe",
        majorVersion: 21,
        architecture: "x64",
        vendor: "Adoptium",
      },
      {
        executable: "/usr/lib/jvm/java-17/bin/java",
        majorVersion: 17,
        architecture: "arm64",
        vendor: null,
      },
    ];
    vi.mocked(invoke).mockResolvedValue(runtimes);

    await expect(listJavaRuntimes()).resolves.toEqual(runtimes);
    expect(invoke).toHaveBeenCalledWith("list_java_runtimes");
  });

  it("accepts no installations and propagates IPC errors", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await expect(listJavaRuntimes()).resolves.toEqual([]);

    vi.mocked(invoke).mockRejectedValueOnce("Java discovery task failed");
    await expect(listJavaRuntimes()).rejects.toBe("Java discovery task failed");
  });
});
