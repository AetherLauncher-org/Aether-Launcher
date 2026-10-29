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
    start_installed_instance: {
      id: string;
      offlineName: string;
      memoryMib: number;
      accountUuid: string | null;
      /** 微软账户的 ID；有它时后端用刷新后的正版令牌启动，忽略玩家名和 UUID */
      accountId: string | null;
    };
    stop_installed_instance: { id: string };
    game_status: { id: string };
  }

  interface CommandResults {
    start_installed_instance: GameRunStatus;
    stop_installed_instance: GameRunStatus;
    game_status: GameRunStatus | null;
  }
}

/**
 * 启动一个已安装的实例。
 *
 * 离线：`accountUuid` 为 null 时由后端用玩家名生成标准离线 UUID；否则必须是 32 位
 * 十六进制（可带连字符）。微软账户：传 `accountId`，后端刷新登录并用正版令牌启动，
 * 令牌不经过前端。
 */
export function startInstalledInstance(
  id: string,
  offlineName: string,
  memoryMib: number,
  accountUuid: string | null = null,
  accountId: string | null = null,
): Promise<GameRunStatus> {
  return callCommand("start_installed_instance", {
    id,
    offlineName,
    memoryMib,
    accountUuid,
    accountId,
  });
}

export function stopInstalledInstance(id: string): Promise<GameRunStatus> {
  return callCommand("stop_installed_instance", { id });
}

/** 本次会话里该实例的运行状态；没启动过时为 null。 */
export function gameStatus(id: string): Promise<GameRunStatus | null> {
  return callCommand("game_status", { id });
}
