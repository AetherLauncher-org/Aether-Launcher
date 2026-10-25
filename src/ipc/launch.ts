import { callCommand } from "./call";

export type GameRunStatus = {
  instanceId: string;
  state: "running" | "exited" | "stopped" | "failed";
  pid: number;
  exitCode: number | null;
  discardedLogs: number;
};

declare module "./call" {
  interface CommandArgs {
    activate_installed_instance: { id: string };
    start_installed_instance: {
      id: string;
      offlineName: string;
      memoryMib: number;
    };
    stop_installed_instance: { id: string };
  }

  interface CommandResults {
    activate_installed_instance: void;
    start_installed_instance: GameRunStatus;
    stop_installed_instance: GameRunStatus;
  }
}

export function activateInstalledInstance(id: string): Promise<void> {
  return callCommand("activate_installed_instance", { id });
}

export function startInstalledInstance(
  id: string,
  offlineName: string,
  memoryMib: number,
): Promise<GameRunStatus> {
  return callCommand("start_installed_instance", { id, offlineName, memoryMib });
}

export function stopInstalledInstance(id: string): Promise<GameRunStatus> {
  return callCommand("stop_installed_instance", { id });
}
