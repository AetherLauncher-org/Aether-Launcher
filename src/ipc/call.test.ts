import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { callCommand, describeCommandError } from "./call";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("callCommand", () => {
  it("forwards a parameterless command without an argument object", async () => {
    vi.mocked(invoke).mockResolvedValue({ name: "Aether Launcher" });

    await callCommand("application_info");

    expect(invoke).toHaveBeenCalledWith("application_info");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("passes the arguments as a single camelCase object", async () => {
    vi.mocked(invoke).mockResolvedValue(null);

    await callCommand("start_installed_instance", {
      id: "a",
      offlineName: "Steve",
      memoryMib: 4096,
      accountUuid: null,
      accountId: null,
    });

    expect(invoke).toHaveBeenCalledWith("start_installed_instance", {
      id: "a",
      offlineName: "Steve",
      memoryMib: 4096,
      accountUuid: null,
      accountId: null,
    });
  });

  it("returns whatever the command resolved", async () => {
    const listing = {
      instances: [
        { id: "a", name: "Survival", version: "1.21.1", state: "ready" },
      ],
      skipped: [{ id: "b", reason: "unreadable" }],
    };
    vi.mocked(invoke).mockResolvedValue(listing);

    await expect(callCommand("list_instances")).resolves.toEqual(listing);
  });

  it("propagates the rejection from invoke", async () => {
    vi.mocked(invoke).mockRejectedValue("instance store unavailable");

    await expect(callCommand("list_instances")).rejects.toBe(
      "instance store unavailable",
    );
  });
});

describe("tracing calls in development", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  const development = () => {
    vi.stubEnv("MODE", "development");
    vi.stubEnv("DEV", true);
  };

  it("says nothing under test, so test output stays clean", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    vi.mocked(invoke).mockResolvedValue(null);

    await callCommand("list_instances");

    expect(debug).not.toHaveBeenCalled();
  });

  it("logs the command, how long it took and what kind of thing came back", async () => {
    development();
    const debug = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    vi.mocked(invoke).mockResolvedValue({ instances: [], skipped: [] });

    await callCommand("list_instances");
    await Promise.resolve();

    const lines = debug.mock.calls.map((call) => call.join(" "));
    expect(lines[0]).toBe("[ipc] → list_instances()");
    expect(lines[1]).toMatch(/^\[ipc\] ← list_instances ok in \d+ ms, object$/);
  });

  it("logs only the names of the arguments, never their values", async () => {
    development();
    const debug = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    vi.mocked(invoke).mockResolvedValue(null);

    await callCommand("get_world_details", { id: "secret-instance-id", world: "Secret World" });
    await Promise.resolve();

    const text = debug.mock.calls.map((call) => call.join(" ")).join("\n");
    expect(text).toContain("get_world_details(id,world)");
    expect(text).not.toContain("secret-instance-id");
    expect(text).not.toContain("Secret World");
  });

  it("logs a failure, and still hands the failure to the caller", async () => {
    development();
    const debug = vi.spyOn(console, "debug").mockImplementation(() => undefined);
    vi.mocked(invoke).mockRejectedValue("no such world");

    await expect(callCommand("list_instances")).rejects.toBe("no such world");
    await Promise.resolve();

    expect(debug.mock.calls.some((call) => String(call[0]).startsWith("[ipc] ✗ list_instances failed"))).toBe(true);
  });

  it("warns once when a call has not answered after five seconds, and not after it has", async () => {
    development();
    vi.useFakeTimers();
    vi.spyOn(console, "debug").mockImplementation(() => undefined);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    let answer: (value: unknown) => void = () => undefined;
    vi.mocked(invoke).mockReturnValue(new Promise((resolve) => (answer = resolve)));

    const pending = callCommand("list_instances");
    await vi.advanceTimersByTimeAsync(4_900);
    expect(warn).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(String(warn.mock.calls[0][0])).toContain("list_instances has not answered after 5 s");

    answer(null);
    await pending;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(warn).toHaveBeenCalledTimes(1);
  });
});

describe("describeCommandError", () => {
  it("keeps a string rejection as-is", () => {
    expect(describeCommandError("cannot list instances")).toBe(
      "cannot list instances",
    );
  });

  it("unwraps an Error", () => {
    expect(describeCommandError(new Error("offline"))).toBe("offline");
  });

  it("reads the message of a plain object", () => {
    expect(describeCommandError({ message: "denied" })).toBe("denied");
  });

  it("never returns an empty or object-ish string", () => {
    expect(describeCommandError(null)).toBe("未知错误");
    expect(describeCommandError(undefined)).toBe("未知错误");
    expect(describeCommandError({ message: "" })).toBe("[object Object]");
    expect(describeCommandError(42)).toBe("42");
  });
});
