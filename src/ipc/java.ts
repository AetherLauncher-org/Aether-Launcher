import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * Java 运行时发现。
 *
 * `list_java_runtimes` 命令由 PR #12 提供，字段与 `JavaRuntimeInfo` 一一对应。
 * 主 lead 合并后，`src/ipc/index.ts` 会重新导出同名函数与类型；本文件与它
 * **签名等价**，合并时保留其中一份即可（见汇报里的 re-export 说明）。
 *
 * 命令目前可能还不存在：非 Tauri 环境、或者桌面版还没注册该命令时，
 * `invoke` 会失败。这两种情况都降级为空列表，让设置页显示「暂未检测到 Java」，
 * 而不是白屏或抛错到界面上。
 */

declare module "./call" {
  interface CommandArgs {
    list_java_runtimes: void;
  }

  interface CommandResults {
    list_java_runtimes: JavaRuntimeInfo[];
  }
}

/** 与 `src-tauri` 的 `JavaRuntimeInfo` 一致：camelCase、`vendor` 可为 null。 */
export type JavaRuntimeInfo = {
  executable: string;
  majorVersion: number;
  architecture: string;
  vendor: string | null;
};

/**
 * 探测本机的 Java 安装。
 *
 * 任何失败都当作「没有发现」处理：设置页只需要一个列表，不需要区分
 * 「命令不存在」和「扫描失败」——两种情况用户能做的事是一样的。
 */
export async function discoverJavaRuntimes(): Promise<JavaRuntimeInfo[]> {
  if (!isTauri()) {
    return [];
  }
  try {
    return await callCommand("list_java_runtimes");
  } catch {
    return [];
  }
}
