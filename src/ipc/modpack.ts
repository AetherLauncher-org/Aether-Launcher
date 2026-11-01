import { callCommand } from "./call";
import { isTauri } from "./index";
import type { InstanceSummary } from "./instances";

export type ModpackRequest = { projectId: string | null; optional: boolean };

declare module "./call" {
  interface CommandArgs {
    install_modpack: ModpackRequest & { name: string };
  }
  interface CommandResults {
    install_modpack: InstanceSummary;
  }
}

/** A null project opens the native picker; paths never cross IPC. */
export function installModpack(projectId: string | null, name: string, optional: boolean): Promise<InstanceSummary> {
  if (!isTauri()) {
    return Promise.reject(new Error("需要桌面版才能安装整合包"));
  }
  return callCommand("install_modpack", { projectId, name: name.trim(), optional });
}
