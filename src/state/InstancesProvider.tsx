import { useMemo } from "react";
import type { ReactNode } from "react";
import { isTauri, listInstances } from "../ipc";
import { useQuery } from "../lib/query";
import { InstancesContext } from "./instances";
import type { InstancesApi } from "./instances";

const noInstances: InstancesApi["instances"] = [];
const noSkipped: InstancesApi["skipped"] = [];

/** 实例列表只读一次并共享：主页、实例页、下载页都用同一份，安装完成后统一刷新。 */
export function InstancesProvider({ children }: { children: ReactNode }) {
  const desktop = isTauri();
  const { status, data, error, reload } = useQuery(listInstances, desktop);

  const api = useMemo<InstancesApi>(
    () => ({
      desktop,
      status,
      instances: data?.instances ?? noInstances,
      skipped: data?.skipped ?? noSkipped,
      error,
      reload,
    }),
    [desktop, status, data, error, reload],
  );

  return <InstancesContext.Provider value={api}>{children}</InstancesContext.Provider>;
}
