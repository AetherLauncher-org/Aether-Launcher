import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { gameStatus, startInstalledInstance, stopInstalledInstance } from "./launch";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

const running = { instanceId: "a", state: "running", pid: 1, exitCode: null, discardedLogs: 0 };

describe("startInstalledInstance", () => {
  it("lets the backend derive the UUID when no account UUID is given", async () => {
    vi.mocked(invoke).mockResolvedValue(running);

    await expect(startInstalledInstance("a", "Steve", 4096)).resolves.toEqual(running);

    expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
      id: "a",
      offlineName: "Steve",
      memoryMib: 4096,
      accountUuid: null,
    });
  });

  it("passes a stored account UUID through", async () => {
    vi.mocked(invoke).mockResolvedValue(running);
    const uuid = "0123456789abcdef0123456789abcdef";

    await startInstalledInstance("a", "Steve", 2048, uuid);

    expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
      id: "a",
      offlineName: "Steve",
      memoryMib: 2048,
      accountUuid: uuid,
    });
  });

  it("never sends paths, URLs or Java arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(running);
    await startInstalledInstance("a", "Steve", 4096);
    const sent = vi.mocked(invoke).mock.calls[0][1] as Record<string, unknown>;
    expect(Object.keys(sent).sort()).toEqual(["accountUuid", "id", "memoryMib", "offlineName"]);
  });
});

describe("stopInstalledInstance and gameStatus", () => {
  it("stops by instance id only", async () => {
    vi.mocked(invoke).mockResolvedValue({ ...running, state: "stopped" });
    await stopInstalledInstance("a");
    expect(invoke).toHaveBeenCalledWith("stop_installed_instance", { id: "a" });
  });

  it("returns null for an instance this session never started", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await expect(gameStatus("a")).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("game_status", { id: "a" });
  });
});
