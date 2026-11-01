import { beforeEach, describe, expect, it, vi } from "vitest";
import { beginContentInstall, cancelContentInstall, listenContentInstallProgress, waitContentInstall } from "../ipc";
import type { ContentInstallArgs, ContentInstallProgress, ContentInstallResult } from "../ipc";
import { createContentInstallStore } from "./contentInstall";

vi.mock("../ipc", () => ({
  beginContentInstall: vi.fn(), cancelContentInstall: vi.fn(),
  listenContentInstallProgress: vi.fn(), waitContentInstall: vi.fn(),
  describeCommandError: (error: unknown) => String(error),
}));
const args = (projectId: string): ContentInstallArgs & { kind: "install" } => ({
  kind: "install",
  id: "instance", projectId, projectType: "mod", world: null, withDependencies: true,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
const result = (operationId: string, status: ContentInstallResult["status"] = "done"): ContentInstallResult => ({
  operationId, status, installed: [], skipped: [], error: null,
});
const tick = async () => { for (let index = 0; index < 12; index += 1) await Promise.resolve(); };

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(listenContentInstallProgress).mockResolvedValue(vi.fn());
  vi.mocked(beginContentInstall).mockImplementation(async (input) => input.projectId);
  vi.mocked(cancelContentInstall).mockResolvedValue(true);
});

describe("content installation store", () => {
  it("reserves at most two slots synchronously, deduplicates rapid clicks and starts queued work only after wait", async () => {
    const pending = new Map(["a", "b", "c"].map((id) => [id, deferred<ContentInstallResult>()]));
    vi.mocked(waitContentInstall).mockImplementation((id) => pending.get(id)!.promise);
    const store = createContentInstallStore();
    store.enqueue("A", args("a"));
    store.enqueue("A", args("a"));
    store.enqueue("B", args("b"));
    store.enqueue("C", args("c"));
    await tick();
    expect(beginContentInstall).toHaveBeenCalledTimes(2);
    expect(store.getSnapshot().map((task) => task.status)).toEqual(["running", "running", "queued"]);
    const onProgress = vi.mocked(listenContentInstallProgress).mock.calls[0][0];
    onProgress({ operationId: "a", phase: "done", projectId: "a", fileName: null, bytesDone: 0, bytesTotal: 0, filesDone: 1, filesDiscovered: 1 });
    await tick();
    expect(beginContentInstall).toHaveBeenCalledTimes(2);
    pending.get("a")!.resolve(result("a"));
    await tick();
    expect(beginContentInstall).toHaveBeenCalledTimes(3);
    pending.get("b")!.resolve(result("b"));
    pending.get("c")!.resolve(result("c"));
    await tick();
  });

  it("buffers early progress and forwards cancellation requested during begin", async () => {
    const beginning = deferred<string>();
    const terminal = deferred<ContentInstallResult>();
    const stop = vi.fn();
    vi.mocked(listenContentInstallProgress).mockResolvedValue(stop);
    vi.mocked(beginContentInstall).mockReturnValue(beginning.promise);
    vi.mocked(waitContentInstall).mockReturnValue(terminal.promise);
    const store = createContentInstallStore();
    store.enqueue("A", args("a"));
    await tick();
    const progress: ContentInstallProgress = { operationId: "opaque", phase: "downloading", projectId: "a", fileName: "a.jar", bytesDone: 5, bytesTotal: 10, filesDone: 0, filesDiscovered: 1 };
    vi.mocked(listenContentInstallProgress).mock.calls[0][0](progress);
    store.cancel(store.getSnapshot()[0].key);
    expect(cancelContentInstall).not.toHaveBeenCalled();
    beginning.resolve("opaque");
    await tick();
    expect(store.getSnapshot()[0].progress).toEqual(progress);
    expect(cancelContentInstall).toHaveBeenCalledWith("opaque");
    expect(stop).not.toHaveBeenCalled();
    terminal.resolve({ ...result("opaque", "cancelled"), installed: [{ projectId: "dep", fileName: "verified.jar", version: "1" }] });
    await tick();
    expect(store.getSnapshot()[0].result?.installed[0].fileName).toBe("verified.jar");
    expect(stop).toHaveBeenCalledOnce();
  });

  it("cancels queued work locally without starting or cancelling a backend worker", async () => {
    const terminal = deferred<ContentInstallResult>();
    vi.mocked(waitContentInstall).mockReturnValue(terminal.promise);
    const store = createContentInstallStore();
    for (const id of ["a", "b", "c"]) store.enqueue(id, args(id));
    store.cancel(store.getSnapshot()[2].key);
    await tick();
    expect(cancelContentInstall).not.toHaveBeenCalled();
    terminal.resolve(result("all"));
    await tick();
    expect(beginContentInstall).toHaveBeenCalledTimes(2);
    expect(store.getSnapshot()[2].status).toBe("cancelled");
  });

  it("keeps running through subscriber teardown and recovers final results if listening fails", async () => {
    vi.mocked(listenContentInstallProgress).mockRejectedValue(new Error("offline events"));
    const terminal = deferred<ContentInstallResult>();
    vi.mocked(waitContentInstall).mockReturnValue(terminal.promise);
    const store = createContentInstallStore();
    const notify = vi.fn();
    const unsubscribe = store.subscribe(notify);
    store.enqueue("A", args("a"));
    await tick();
    expect(store.getSnapshot()[0].notice).toMatch(/进度订阅失败/);
    unsubscribe();
    terminal.resolve(result("a"));
    await tick();
    expect(store.getSnapshot()[0].status).toBe("done");
    const remount = vi.fn();
    const unmount = store.subscribe(remount);
    store.dismiss(store.getSnapshot()[0].key);
    expect(remount).toHaveBeenCalledOnce();
    unmount();
  });
});
