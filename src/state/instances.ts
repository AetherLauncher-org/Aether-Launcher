import { createContext, useContext } from "react";
import type { InstanceSummary, SkippedInstance } from "../ipc";
import type { QueryStatus } from "../lib/query";

export type InstancesApi = {
  /** 浏览器预览里没有桌面后端，列表恒为空 */
  desktop: boolean;
  status: QueryStatus;
  instances: InstanceSummary[];
  skipped: SkippedInstance[];
  error: string | null;
  reload: () => void;
};

export const InstancesContext = createContext<InstancesApi | null>(null);

export function useInstances(): InstancesApi {
  const value = useContext(InstancesContext);
  if (value === null) {
    throw new Error("useInstances 需要在 InstancesProvider 内使用");
  }
  return value;
}
