import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  describeCommandError,
  gameStatus,
  startInstalledInstance,
  stopInstalledInstance,
} from "../ipc";
import type { InstanceSummary } from "../ipc";
import {
  failedSession,
  idleSession,
  sessionFromStatus,
  startingSession,
  statusPollIntervalMs,
} from "../lib/game";
import type { GameSession } from "../lib/game";
import { resolveLaunchMemoryMiB } from "../lib/launchMemory";
import { saveLastPlayedId } from "../lib/settings";
import { useAccounts } from "./accounts";
import { GameContext } from "./game";
import type { GameApi } from "./game";

/**
 * 游戏会话放在应用层：切换页面不会丢失「运行中」状态，
 * 并且在有游戏运行时轮询后端，游戏退出后界面能自己更新。
 */
export function GameProvider({ children }: { children: ReactNode }) {
  const { active } = useAccounts();
  const [sessions, setSessions] = useState<Record<string, GameSession>>({});
  const activeRef = useRef(active);

  useEffect(() => {
    activeRef.current = active;
  }, [active]);

  const setSession = useCallback((id: string, session: GameSession) => {
    setSessions((previous) => ({ ...previous, [id]: session }));
  }, []);

  const start = useCallback(
    async (instance: InstanceSummary) => {
      const account = activeRef.current;
      if (account === null) {
        setSession(instance.id, failedSession("请先添加并选择一个账户"));
        return;
      }
      setSession(instance.id, startingSession);
      try {
        const status = await startInstalledInstance(
          instance.id,
          account.name,
          await resolveLaunchMemoryMiB(),
          account.uuid,
        );
        saveLastPlayedId(instance.id);
        setSession(instance.id, sessionFromStatus(status));
      } catch (error: unknown) {
        setSession(instance.id, failedSession(describeCommandError(error)));
      }
    },
    [setSession],
  );

  const stop = useCallback(
    async (id: string) => {
      try {
        setSession(id, sessionFromStatus(await stopInstalledInstance(id)));
      } catch (error: unknown) {
        setSession(id, failedSession(describeCommandError(error)));
      }
    },
    [setSession],
  );

  const runningIds = useMemo(
    () =>
      Object.entries(sessions)
        .filter(([, session]) => session.phase === "running")
        .map(([id]) => id)
        .sort()
        .join(","),
    [sessions],
  );

  useEffect(() => {
    if (runningIds.length === 0) {
      return;
    }
    const ids = runningIds.split(",");
    let cancelled = false;
    const poll = async () => {
      for (const id of ids) {
        try {
          const status = await gameStatus(id);
          if (cancelled) {
            return;
          }
          // 后端不认识这个实例（例如应用重启过），当作已退出
          setSession(id, status === null ? { ...idleSession, phase: "exited" } : sessionFromStatus(status));
        } catch {
          // 下一轮再试；轮询失败不应该打断界面
        }
      }
    };
    const timer = window.setInterval(() => void poll(), statusPollIntervalMs);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [runningIds, setSession]);

  const api = useMemo<GameApi>(
    () => ({
      sessionOf: (id: string) => sessions[id] ?? idleSession,
      start,
      stop,
      anyRunning: runningIds.length > 0,
    }),
    [sessions, start, stop, runningIds],
  );

  return <GameContext.Provider value={api}>{children}</GameContext.Provider>;
}
