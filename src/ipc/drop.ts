import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { callCommand } from "./call";
import { isTauri } from "./index";

/** 路径只留在后端；令牌有效两分钟且只能使用一次。 */
export type DroppedItem = { name: string; size: number; kind: "mod" | "datapack" | "resourcepack" | "shader" };
export type DropPreview = { token: string; items: DroppedItem[] };
export type DropReport = { installed: { name: string; world: string | null }[]; skipped: { name: string; world: string | null; reason: string }[] };
declare module "./call" {
  interface CommandArgs { install_drop: { token: string; id: string; worlds: string[] } }
  interface CommandResults { install_drop: DropReport }
}
export function installDrop(token: string, id: string, worlds: string[]): Promise<DropReport> {
  if (!isTauri()) return Promise.reject(new Error("需要桌面版才能安装"));
  return callCommand("install_drop", { token, id, worlds });
}
export async function listenDrop(onPreview: (preview: DropPreview) => void, onRejected: (message: string) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  const unlistenPreview = await listen<DropPreview>("drop://staged", (event) => onPreview(event.payload));
  try {
    const unlistenError = await listen<string>("drop://rejected", (event) => onRejected(event.payload));
    return () => { unlistenPreview(); unlistenError(); };
  } catch (error) { unlistenPreview(); throw error; }
}
