import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  beginMicrosoftBrowserLogin,
  beginMicrosoftLogin,
  cancelMicrosoftLogin,
  listMicrosoftAccounts,
  listenLoginStatus,
  removeMicrosoftAccount,
} from "../ipc/accounts";
import type { LoginEvent } from "../ipc/accounts";
import { callCommand } from "../ipc/call";
import type { InstallProgress } from "../ipc/install";
import { installMockBackend } from "./mockBackend";

beforeAll(() => {
  installMockBackend();
});

beforeEach(() => {
  vi.useFakeTimers();
  window.localStorage.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("the browser mock backend", () => {
  it("makes the frontend believe it runs in Tauri and serves the read-only commands", async () => {
    expect("__TAURI_INTERNALS__" in window).toBe(true);

    const listing = await callCommand("list_instances");
    expect(listing.instances.length).toBeGreaterThan(0);

    const memory = await callCommand("system_memory");
    expect(memory?.totalMib).toBeGreaterThan(0);

    const catalog = callCommand("list_minecraft_versions");
    await vi.advanceTimersByTimeAsync(300);
    expect((await catalog).versions.some((version) => version.type === "release")).toBe(true);
  });

  it("rejects commands it does not know so a typo cannot pass silently", async () => {
    await expect(
      (callCommand as (name: string) => Promise<unknown>)("definitely_not_a_command"),
    ).rejects.toMatch(/unknown command/);
  });

  it("installs a version with progress events and adds the instance", async () => {
    const seen: InstallProgress[] = [];
    const unlisten = await listen<InstallProgress>("install://progress", (event) =>
      seen.push(event.payload),
    );

    const before = (await callCommand("list_instances")).instances.length;
    const installed = callCommand("install_vanilla_version", { version: "1.21.4", name: "Mock 1.21.4" });
    await vi.advanceTimersByTimeAsync(10_000);

    expect((await installed).state).toBe("ready");
    expect(seen[seen.length - 1]?.stage).toBe("finalize");
    expect(seen.map((progress) => progress.stage)).toEqual(
      expect.arrayContaining(["metadata", "files", "assets", "finalize"]),
    );
    expect((await callCommand("list_instances")).instances).toHaveLength(before + 1);
    unlisten();
  });

  it("refuses a second install while one is running and honours cancel", async () => {
    const first = callCommand("install_vanilla_version", { version: "1.21", name: "Cancelled one" });
    first.catch(() => undefined);
    await vi.advanceTimersByTimeAsync(150);

    await expect(
      callCommand("install_vanilla_version", { version: "1.20.1", name: "Second" }),
    ).rejects.toMatch(/already running/);

    expect(await callCommand("cancel_vanilla_install")).toBe(true);
    await vi.advanceTimersByTimeAsync(500);
    await expect(first).rejects.toBe("cancelled");
  });

  it("walks a Microsoft sign-in from code to account and back out", async () => {
    const events: LoginEvent[] = [];
    const unlisten = await listenLoginStatus((event) => events.push(event));
    expect(await listMicrosoftAccounts()).toEqual([]);

    const code = await beginMicrosoftLogin();
    expect(code.userCode).toBeTruthy();
    await expect(beginMicrosoftLogin()).rejects.toMatch(/already in progress/);

    await vi.advanceTimersByTimeAsync(7000);

    expect(events.map((event) => event.state)).toEqual(["waiting", "signed-in"]);
    const accounts = await listMicrosoftAccounts();
    expect(accounts).toHaveLength(1);
    expect(JSON.stringify(accounts)).not.toMatch(/token/i);

    await removeMicrosoftAccount(accounts![0].id);
    expect(await listMicrosoftAccounts()).toEqual([]);
    unlisten();
  });

  it("walks a browser sign-in without any code to show", async () => {
    const events: LoginEvent[] = [];
    const unlisten = await listenLoginStatus((event) => events.push(event));

    const info = await beginMicrosoftBrowserLogin();
    expect(info.expiresInSecs).toBeGreaterThan(0);
    // Only one sign-in at a time, whichever method started it.
    await expect(beginMicrosoftLogin()).rejects.toMatch(/already in progress/);

    await vi.advanceTimersByTimeAsync(7000);
    expect(events.map((event) => event.state)).toEqual(["waiting", "signed-in"]);
    unlisten();
    await removeMicrosoftAccount("069a79f4-44e9-4726-a5be-fca90e38aaf5");
  });

  it("reports a cancelled sign-in", async () => {
    const events: LoginEvent[] = [];
    const unlisten = await listenLoginStatus((event) => events.push(event));

    await beginMicrosoftLogin();
    expect(await cancelMicrosoftLogin()).toBe(true);
    await vi.advanceTimersByTimeAsync(7000);

    expect(events[events.length - 1]?.state).toBe("cancelled");
    expect(await listMicrosoftAccounts()).toEqual([]);
    unlisten();
  });

  it("starts a game as a signed-in Microsoft account only if that account exists", async () => {
    const { instances } = await callCommand("list_instances");
    const id = instances[0].id;

    await expect(
      callCommand("start_installed_instance", {
        id,
        offlineName: "x",
        memoryMib: 2048,
        accountUuid: null,
        accountId: "nobody",
      }),
    ).rejects.toBe("microsoft-sign-in-required");

    const status = await callCommand("start_installed_instance", {
      id,
      offlineName: "Steve",
      memoryMib: 2048,
      accountUuid: null,
      accountId: null,
    });
    expect(status.state).toBe("running");
    expect((await callCommand("stop_installed_instance", { id })).state).toBe("stopped");
  });

  it("imports the selected versions and skips loader ones", async () => {
    const picked = callCommand("pick_import_folder");
    await vi.advanceTimersByTimeAsync(400);
    const source = await picked;
    const result = callCommand("import_versions", {
      token: source!.token,
      ids: ["1.20.1", "1.20.1-fabric"],
    });
    await vi.advanceTimersByTimeAsync(2000);

    const done = await result;
    expect(done.imported.map((instance) => instance.version)).toEqual(["1.20.1"]);
    expect(done.skipped).toEqual([{ id: "1.20.1-fabric", reason: "needs-loader" }]);
  });
});
