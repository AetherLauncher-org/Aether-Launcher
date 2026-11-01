import { useSyncExternalStore } from "react";
import {
  beginContentInstall, beginFolderSave, cancelContentInstall, describeCommandError,
  listenContentInstallProgress, waitContentInstall,
} from "../ipc";
import type { ContentInstallArgs, ContentInstallProgress, ContentInstallResult, FolderSaveArgs } from "../ipc";

export type ContentTaskArgs =
  | ({ kind: "install" } & ContentInstallArgs)
  | ({ kind: "folder" } & FolderSaveArgs);

export type ContentInstallTask = {
  key: number;
  title: string;
  args: ContentTaskArgs;
  status: "queued" | "running" | ContentInstallResult["status"];
  operationId: string | null;
  progress: ContentInstallProgress | null;
  result: ContentInstallResult | null;
  cancelRequested: boolean;
  notice: string | null;
};

const sameTarget = (left: ContentTaskArgs, right: ContentTaskArgs) => {
  if (left.kind !== right.kind) return false;
  if (left.id !== right.id || left.projectId !== right.projectId || left.projectType !== right.projectType) return false;
  if (left.kind === "install" && right.kind === "install") return left.world === right.world;
  return true;
};

// Module lifetime deliberately exceeds ContentBrowser mounts and download tabs.
export function createContentInstallStore() {
  let tasks: readonly ContentInstallTask[] = [];
  let nextKey = 1;
  let active = 0;
  let listener: Promise<void> | null = null;
  let stop: (() => void) | null = null;
  let listeningError: string | null = null;
  const early = new Map<string, ContentInstallProgress>();
  const subscribers = new Set<() => void>();
  const publish = () => subscribers.forEach((notify) => notify());
  const patch = (key: number, update: Partial<ContentInstallTask>) => {
    tasks = tasks.map((task) => task.key === key ? { ...task, ...update } : task);
    publish();
  };
  const get = (key: number) => tasks.find((task) => task.key === key)!;
  const ensureListener = () => {
    listener ??= listenContentInstallProgress((progress) => {
      const task = tasks.find((item) => item.operationId === progress.operationId && item.status === "running");
      if (task) patch(task.key, { progress });
      else if (tasks.some((item) => item.status === "running" && item.operationId === null)) {
        // A worker can emit before begin returns its opaque ID. Keep only latest snapshots.
        if (early.size >= 128) early.delete(early.keys().next().value!);
        early.set(progress.operationId, progress);
      }
    }).then((unlisten) => { stop = unlisten; }).catch((error: unknown) => {
      listeningError = `进度订阅失败：${describeCommandError(error)}；仍会等待安装结果。`;
    });
    return listener;
  };
  const releaseListener = () => {
    if (active !== 0) return;
    stop?.();
    stop = null;
    listener = null;
    listeningError = null;
    early.clear();
  };
  const requestCancel = async (key: number) => {
    const task = get(key);
    if (task.operationId === null) return;
    try {
      const accepted = await cancelContentInstall(task.operationId);
      if (!accepted && get(key)?.status === "running") {
        patch(key, { notice: "操作已结束，正在读取结果。" });
      }
    } catch (error: unknown) {
      if (get(key)?.status === "running") patch(key, { cancelRequested: false, notice: `取消失败：${describeCommandError(error)}` });
    }
  };
  // `kind` is a frontend-only discriminant: strip it so the backend sees only the command's own args.
  const begin = async (task: ContentInstallTask): Promise<string> => {
    const { kind, ...payload } = task.args;
    if (kind === "install") {
      return beginContentInstall(payload as ContentInstallArgs);
    }
    return beginFolderSave(payload as FolderSaveArgs);
  };
  const run = async (key: number) => {
    try {
      await ensureListener();
      patch(key, { notice: listeningError });
      const operationId = await begin(get(key));
      patch(key, { operationId, progress: early.get(operationId) ?? null });
      early.delete(operationId);
      // Cancellation during begin is deferred until the backend ID becomes available.
      if (get(key).cancelRequested) void requestCancel(key);
      const result = await waitContentInstall(operationId);
      patch(key, { status: result.status, result, notice: null });
    } catch (error: unknown) {
      patch(key, { status: "failed", notice: describeCommandError(error) });
    } finally {
      active -= 1;
      pump();
      releaseListener();
    }
  };
  const pump = () => {
    for (const task of tasks) {
      if (active >= 2) break;
      if (task.status !== "queued") continue;
      // Reserve synchronously before invoking any async work, including listener setup.
      active += 1;
      patch(task.key, { status: "running" });
      void run(task.key);
    }
  };
  return {
    subscribe(notify: () => void) { subscribers.add(notify); return () => { subscribers.delete(notify); }; },
    getSnapshot: () => tasks,
    enqueue(title: string, args: ContentTaskArgs) {
      if (tasks.some((task) => sameTarget(task.args, args) &&
        (task.status === "queued" || task.status === "running"))) return;
      tasks = [...tasks, {
        key: nextKey++, title, args, status: "queued", operationId: null, progress: null,
        result: null, cancelRequested: false, notice: null,
      }];
      publish();
      pump();
    },
    cancel(key: number) {
      const task = get(key);
      if (task.status === "queued") {
        patch(key, { status: "cancelled" });
      } else if (task.status === "running" && !task.cancelRequested) {
        patch(key, { cancelRequested: true, notice: null });
        void requestCancel(key);
      }
    },
    dismiss(key: number) {
      tasks = tasks.filter((task) => task.key !== key || task.status === "queued" || task.status === "running");
      publish();
    },
  };
}

export const contentInstallStore = createContentInstallStore();

export function useContentInstalls() {
  const tasks = useSyncExternalStore(contentInstallStore.subscribe, contentInstallStore.getSnapshot);
  return { tasks, enqueue: contentInstallStore.enqueue, cancel: contentInstallStore.cancel, dismiss: contentInstallStore.dismiss };
}
