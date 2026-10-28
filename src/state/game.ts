import { createContext, useContext } from "react";
import type { InstanceSummary } from "../ipc";
import type { GameSession } from "../lib/game";

export type GameApi = {
  /** 某个实例本次会话里的状态；没启动过时是空闲 */
  sessionOf: (id: string) => GameSession;
  /** 用当前账户和默认内存启动实例。没有账户时把会话置为失败并说明原因。 */
  start: (instance: InstanceSummary) => Promise<void>;
  stop: (id: string) => Promise<void>;
  /** 是否有任何实例正在运行 */
  anyRunning: boolean;
};

export const GameContext = createContext<GameApi | null>(null);

export function useGame(): GameApi {
  const value = useContext(GameContext);
  if (value === null) {
    throw new Error("useGame 需要在 GameProvider 内使用");
  }
  return value;
}
