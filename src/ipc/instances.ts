import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 实例相关命令的类型化封装。
 *
 * 命令定义在 `src-tauri/src/commands/instances.rs`：
 * - `list_instances` 无参数，返回 `{ instances, skipped }`。
 * - `create_instance` 接收 `name` 与 `version`，返回新建的实例摘要。
 *
 * 后端返回的是实例摘要，没有内存、Java 或磁盘路径字段——这些属于本机设置，
 * 存在 `localStorage`（`aether.memoryMiB`、`aether.javaRuntime`），不要指望 IPC 给。
 *
 * 本文件不修改 `src/ipc/index.ts`。合并时由主 lead 在 `index.ts` 里补上
 * 对外的 re-export，并把下面两张登记表合并进 `CommandArgs` / `CommandResults`。
 */

declare module "./call" {
  interface CommandArgs {
    list_instances: void;
  }

  interface CommandResults {
    list_instances: InstanceListing;
  }
}

/** 与 `aether_core::InstanceState` 的蛇形标签一一对应。 */
export type InstanceState = "ready" | "incomplete" | "running" | "failed";

export type InstanceSummary = {
  id: string;
  name: string;
  version: string;
  state: InstanceState;
};

/** 读取失败的记录，只带稳定分类，不带文件系统路径。 */
export type SkippedInstance = {
  id: string;
  reason: string;
};

export type InstanceListing = {
  instances: InstanceSummary[];
  skipped: SkippedInstance[];
};

export const emptyInstanceListing: InstanceListing = {
  instances: [],
  skipped: [],
};

/**
 * 列出本机实例。
 *
 * 在浏览器里单独跑前端时没有 Tauri 运行时，直接返回空列表——页面负责提示
 * 「需要桌面版」，这里不抛错，避免把浏览器预览变成一个红色错误页。
 */
export async function listInstances(): Promise<InstanceListing> {
  if (!isTauri()) {
    return emptyInstanceListing;
  }
  return callCommand("list_instances");
}
