import { createContext, useContext } from "react";
import type { InstallProgress, InstanceSummary, ModLoaderKind } from "../ipc";

export type InstallStatus = "running" | "done" | "error" | "cancelled";

/** 这次安装要装的模组加载器；`null` 表示只装原版。 */
export type InstallLoader = {
  kind: ModLoaderKind;
  /** 加载器自己的版本号，例如 Fabric 的 `0.16.9` */
  version: string;
};

/** 当前（或刚结束的）一次安装。同一时间只有一个，后端也只允许一个。 */
export type InstallJob = {
  version: string;
  name: string;
  /** 装的是哪个加载器。`null` 是原版。进度条和重试都靠它区分两种安装。 */
  loader: InstallLoader | null;
  status: InstallStatus;
  progress: InstallProgress | null;
  /** 字节/秒，指数滑动平均；未知时为 0 */
  speed: number;
  instance: InstanceSummary | null;
  error: string | null;
};

export type InstallApi = {
  job: InstallJob | null;
  /** 开始安装并创建实例。`loader` 为空时只装原版。已有安装在跑时忽略。 */
  start: (
    version: string,
    name: string,
    loader?: InstallLoader | null,
  ) => Promise<void>;
  /**
   * 按上一次安装的目标（版本、实例名、加载器）再装一次。没有安装记录、或者
   * 已经有安装在跑时什么都不做。重试必须沿用同一份目标：漏掉加载器会把「装了一半的
   * Fabric」悄悄变成装原版。
   */
  retry: () => void;
  cancel: () => void;
  /** 清掉已结束的安装记录（完成、失败或取消后）。 */
  dismiss: () => void;
};

export const InstallContext = createContext<InstallApi | null>(null);

export function useInstall(): InstallApi {
  const value = useContext(InstallContext);
  if (value === null) {
    throw new Error("useInstall 需要在 InstallProvider 内使用");
  }
  return value;
}
