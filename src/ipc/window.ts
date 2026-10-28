import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "./index";

/**
 * 无边框窗口的控制。窗口由 Tauri 创建时不带系统标题栏，最小化、最大化、
 * 关闭和拖动都由界面自己画出来，再调用这里的函数。
 *
 * 只开放这几个动作（权限见 `capabilities/default.json`），
 * 不提供任意窗口操作的入口。浏览器预览里全部是空操作。
 */

export async function minimizeWindow(): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().minimize();
  }
}

export async function toggleMaximizeWindow(): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().toggleMaximize();
  }
}

export async function closeWindow(): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().close();
  }
}

/** 让窗口材质（Mica）跟随界面的明暗，而不是系统的明暗。 */
export async function setWindowTheme(theme: "light" | "dark"): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().setTheme(theme);
  }
}

/**
 * 监听最大化状态，用来切换「最大化／还原」图标。
 * 返回取消监听的函数；浏览器预览里什么都不做。
 */
export async function watchMaximized(
  handler: (maximized: boolean) => void,
): Promise<() => void> {
  if (!isTauri()) {
    return () => undefined;
  }
  const current = getCurrentWindow();
  handler(await current.isMaximized());
  const unlisten = await current.onResized(() => {
    void current.isMaximized().then(handler);
  });
  return unlisten;
}
