import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pretendBrowser, pretendDesktop } from "../test/support";
import { installModpack } from "./modpack";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  pretendBrowser();
});
afterEach(pretendBrowser);

const summary = { id: "pack1", name: "My Pack", version: "1.21.1", state: "ready" };

describe("installModpack IPC contract", () => {
  it("rejects browser preview without invoking native code", async () => {
    await expect(installModpack(null, "My Pack", false)).rejects.toThrow("需要桌面版");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("requests the native picker with null projectId and no path or URL", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockResolvedValue(summary);
    await expect(installModpack(null, "  My Pack  ", false)).resolves.toEqual(summary);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("install_modpack", {
      projectId: null, name: "My Pack", optional: false,
    });
  });

  it.each([false, true])("preserves the remote project and optional=%s", async (optional) => {
    pretendDesktop();
    vi.mocked(invoke).mockResolvedValue(summary);
    await expect(installModpack("Pack0001", " My Pack ", optional)).resolves.toEqual(summary);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("install_modpack", {
      projectId: "Pack0001", name: "My Pack", optional,
    });
  });

  it("preserves native picker cancellation for the install provider", async () => {
    pretendDesktop();
    vi.mocked(invoke).mockRejectedValue("cancelled");
    await expect(installModpack(null, "My Pack", false)).rejects.toBe("cancelled");
  });
});
