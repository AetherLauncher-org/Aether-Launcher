import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelVanillaInstall,
  installProgressEvent,
  isInstallCancelled,
  listenInstallProgress,
  startVanillaInstall,
} from "./install";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function pretendDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("startVanillaInstall", () => {
  it("refuses to run in the browser preview without calling native code", async () => {
    await expect(startVanillaInstall("1.21.1", "Survival")).rejects.toThrow("需要桌面版");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("sends only the version and the instance name, trimmed", async () => {
    pretendDesktop();
    const summary = { id: "a", name: "Survival", version: "1.21.1", state: "ready" };
    vi.mocked(invoke).mockResolvedValue(summary);

    await expect(startVanillaInstall(" 1.21.1 ", " Survival ")).resolves.toEqual(summary);

    expect(invoke).toHaveBeenCalledWith("install_vanilla_version", {
      version: "1.21.1",
      name: "Survival",
    });
  });
});

describe("cancelVanillaInstall", () => {
  it("does nothing in the browser preview", async () => {
    await expect(cancelVanillaInstall()).resolves.toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("asks the backend to cancel", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockResolvedValue(true);
    await expect(cancelVanillaInstall()).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("cancel_vanilla_install");
  });
});

describe("isInstallCancelled", () => {
  it("recognises the backend's cancellation message only", () => {
    expect(isInstallCancelled("cancelled")).toBe(true);
    expect(isInstallCancelled(new Error("cancelled"))).toBe(true);
    expect(isInstallCancelled("2 of 5 files failed")).toBe(false);
    expect(isInstallCancelled(null)).toBe(false);
  });
});

describe("listenInstallProgress", () => {
  it("returns a harmless unsubscribe in the browser preview", async () => {
    const stop = await listenInstallProgress(() => undefined);
    expect(() => stop()).not.toThrow();
    expect(listen).not.toHaveBeenCalled();
  });

  it("forwards each event payload to the handler", async () => {
    pretendDesktop();
    const unlisten = vi.fn();
    let deliver: (event: { payload: unknown }) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (_name, callback) => {
      deliver = callback as typeof deliver;
      return unlisten;
    });
    const received: unknown[] = [];

    const stop = await listenInstallProgress((progress) => received.push(progress));
    const payload = {
      version: "1.21.1",
      stage: "files",
      filesDone: 1,
      filesTotal: 2,
      bytesDone: 10,
      bytesTotal: 20,
    };
    deliver({ payload });

    expect(listen).toHaveBeenCalledWith(installProgressEvent, expect.any(Function));
    expect(received).toEqual([payload]);
    expect(stop).toBe(unlisten);
  });
});
