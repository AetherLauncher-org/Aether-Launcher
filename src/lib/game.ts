import type { GameRunStatus } from "../ipc";

/**
 * 一次游戏会话的纯逻辑：把后端返回的运行状态收敛成界面要用的阶段、
 * 徽章文案和按钮可用性。会话只存在于本次运行内——后端不持久化进程。
 */

export type SessionPhase = "idle" | "starting" | "running" | "stopped" | "exited" | "failed";

export type GameSession = {
  phase: SessionPhase;
  pid: number | null;
  exitCode: number | null;
  /** 启动失败时给用户看的原因；运行中和空闲时为 null */
  error: string | null;
};

export const idleSession: GameSession = {
  phase: "idle",
  pid: null,
  exitCode: null,
  error: null,
};

export const startingSession: GameSession = { ...idleSession, phase: "starting" };

export function failedSession(error: string): GameSession {
  return { ...idleSession, phase: "failed", error };
}

/** 后端状态 → 会话。`exited` 里退出码非 0 当作异常退出。 */
export function sessionFromStatus(status: GameRunStatus): GameSession {
  switch (status.state) {
    case "running":
      return { phase: "running", pid: status.pid, exitCode: null, error: null };
    case "stopped":
      return { phase: "stopped", pid: null, exitCode: status.exitCode, error: null };
    case "exited":
      return { phase: "exited", pid: null, exitCode: status.exitCode, error: null };
    case "failed":
      return {
        phase: "failed",
        pid: null,
        exitCode: status.exitCode,
        error:
          status.exitCode === null ? "游戏进程异常结束" : `游戏异常退出（退出码 ${status.exitCode}）`,
      };
  }
}

export type SessionTone = "success" | "informative" | "warning" | "danger" | "subtle";

export type SessionView = { label: string; tone: SessionTone };

/** 退出码 0 或 null 都算正常结束；其他当作异常。 */
function exitedNormally(session: GameSession): boolean {
  return session.exitCode === null || session.exitCode === 0;
}

export function describeSession(session: GameSession): SessionView {
  switch (session.phase) {
    case "idle":
      return { label: "就绪", tone: "success" };
    case "starting":
      return { label: "启动中", tone: "informative" };
    case "running":
      return { label: "运行中", tone: "informative" };
    case "stopped":
      return { label: "已停止", tone: "subtle" };
    case "exited":
      return exitedNormally(session)
        ? { label: "已退出", tone: "subtle" }
        : { label: `异常退出（${session.exitCode}）`, tone: "danger" };
    case "failed":
      return { label: "启动失败", tone: "danger" };
  }
}

/** 空闲、已结束或失败后都可以再次启动；启动中和运行中不行。 */
export function canStart(session: GameSession): boolean {
  return session.phase !== "starting" && session.phase !== "running";
}

export function canStop(session: GameSession): boolean {
  return session.phase === "running";
}

/** 启动按钮的文案：失败后叫「重试」，其余叫「启动」。 */
export function startLabel(session: GameSession): string {
  switch (session.phase) {
    case "starting":
      return "启动中…";
    case "failed":
      return "重试";
    default:
      return "启动";
  }
}

/** 轮询的间隔。游戏退出后最多晚这么久界面才会更新。 */
export const statusPollIntervalMs = 2000;
