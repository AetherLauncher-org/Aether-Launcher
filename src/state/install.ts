import { createContext, useContext } from "react";
import type { InstallProgress, InstanceSummary } from "../ipc";

export type InstallStatus = "running" | "done" | "error" | "cancelled";

/** 当前（或刚结束的）一次安装。同一时间只有一个，后端也只允许一个。 */
export type InstallJob = {
  version: string;
  name: string;
  status: InstallStatus;
  progress: InstallProgress | null;
  /** 字节/秒，指数滑动平均；未知时为 0 */
  speed: number;
  instance: InstanceSummary | null;
  error: string | null;
};

export type InstallApi = {
  job: InstallJob | null;
  /** 开始安装并创建实例。已有安装在跑时忽略。 */
  start: (version: string, name: string) => Promise<void>;
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
