import { getSystemMemory } from "../ipc";
import { effectiveMemoryMiB } from "./memory";
import { loadMemoryMiB, loadMemoryMode } from "./settings";

/**
 * 启动游戏时用的内存上限。自动模式在每次启动时重新读取本机的可用内存，
 * 而不是用设置页打开时的旧数字——那时候和现在可用内存可能已经差很多。
 */
export async function resolveLaunchMemoryMiB(): Promise<number> {
  const mode = loadMemoryMode();
  const system = mode === "auto" ? await getSystemMemory() : null;
  return effectiveMemoryMiB(mode, loadMemoryMiB(), system);
}
