import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * Java 运行时发现。
 *
 * 扫描结果由后端缓存在 `java-runtimes.json` 里：平时只读缓存，只有用户点
 * 「重新扫描」才真的去扫一遍。扫描要启动一串 `java -version`，每次切页面都
 * 做一遍会让界面反复卡顿。
 */

declare module "./call" {
  interface CommandArgs {
    list_java_runtimes: { refresh?: boolean };
    pick_java_runtime: void;
  }

  interface CommandResults {
    list_java_runtimes: JavaRuntimeInfo[];
    pick_java_runtime: JavaRuntimeInfo | null;
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
 * 本机的 Java 列表。
 *
 * `refresh` 为 true 时重新扫描；否则后端直接给上次缓存的结果（没有缓存时才扫）。
 * 任何失败都当作「没有发现」处理：设置页只需要一个列表。
 */
export async function discoverJavaRuntimes(
  refresh = false,
): Promise<JavaRuntimeInfo[]> {
  if (!isTauri()) {
    return [];
  }
  try {
    return await callCommand("list_java_runtimes", { refresh });
  } catch {
    return [];
  }
}

/**
 * 让用户手动挑一个 `java.exe`（或 Java 安装目录）。
 *
 * 后端打开文件选择框，探测成功后把路径记进缓存，返回探测结果。
 * `null` 表示用户取消了选择；抛错表示选的东西不是能用的 Java。
 */
export async function pickJavaRuntime(): Promise<JavaRuntimeInfo | null> {
  if (!isTauri()) {
    return null;
  }
  return callCommand("pick_java_runtime");
}
