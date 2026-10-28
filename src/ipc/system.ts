import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 本机物理内存（MiB）。`availableMib` 为 null 表示系统没有报告可用内存，
 * 界面不能凭空编一个数字。
 */
export type SystemMemory = {
  totalMib: number;
  availableMib: number | null;
};

declare module "./call" {
  interface CommandArgs {
    system_memory: void;
  }

  interface CommandResults {
    system_memory: SystemMemory | null;
  }
}

/**
 * 读取本机内存。浏览器预览里没有桌面后端，读取失败或系统不报告时都返回 null，
 * 调用方据此回落到不依赖本机内存的默认值。
 */
export async function getSystemMemory(): Promise<SystemMemory | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    return await callCommand("system_memory");
  } catch {
    return null;
  }
}
