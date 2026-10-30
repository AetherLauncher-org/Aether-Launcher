import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { callCommand, describeCommandError } from "./call";
import { isTauri } from "./index";
import type { InstanceSummary } from "./instances";

/**
 * 一键安装的命令封装。
 *
 * 命令定义在 `src-tauri/src/commands/install.rs`。前端只传版本号和实例名；
 * 下载地址、校验值、目标路径都来自后端校验过的官方清单。进度通过
 * `install://progress` 事件推送，不需要轮询。
 */

export type InstallStage = "metadata" | "files" | "assets" | "modLoader" | "finalize";

/** 当前阶段的进度。文件数和字节数都包含「已存在且校验通过」的部分。 */
export type InstallProgress = {
  version: string;
  stage: InstallStage;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
};

declare module "./call" {
  interface CommandArgs {
    install_vanilla_version: { version: string; name: string };
    cancel_vanilla_install: void;
  }

  interface CommandResults {
    install_vanilla_version: InstanceSummary;
    cancel_vanilla_install: boolean;
  }
}

export const installProgressEvent = "install://progress";

/** 后端在用户取消时返回的错误文本。 */
const cancelledMessage = "cancelled";

export function isInstallCancelled(error: unknown): boolean {
  return describeCommandError(error) === cancelledMessage;
}

/** 安装一个官方版本并创建实例。非桌面版会 reject。 */
export function startVanillaInstall(
  version: string,
  name: string,
): Promise<InstanceSummary> {
  if (!isTauri()) {
    return Promise.reject(new Error("需要桌面版才能安装游戏"));
  }
  return callCommand("install_vanilla_version", {
    version: version.trim(),
    name: name.trim(),
  });
}

/** 取消正在进行的安装；已下载的文件会保留，下次继续。 */
export function cancelVanillaInstall(): Promise<boolean> {
  if (!isTauri()) {
    return Promise.resolve(false);
  }
  return callCommand("cancel_vanilla_install");
}

/** 订阅安装进度。浏览器预览里没有事件源，返回一个空的取消函数。 */
export async function listenInstallProgress(
  handler: (progress: InstallProgress) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => undefined;
  }
  return listen<InstallProgress>(installProgressEvent, (event) => handler(event.payload));
}
