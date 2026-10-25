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
    vi.mocked(invoke).mockResolvedValue({
      id: "a",
      name: "Survival",
      version: "1.21.1",
      state: "ready",
    });

    await callCommand("create_instance", {
      name: "Survival",
      version: "1.21.1",
    });

    expect(invoke).toHaveBeenCalledWith("create_instance", {
      name: "Survival",
      version: "1.21.1",
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
