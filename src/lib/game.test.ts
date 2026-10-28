import { describe, expect, it } from "vitest";
import type { GameRunStatus } from "../ipc";
import {
  canStart,
  canStop,
  describeSession,
  failedSession,
  idleSession,
  sessionFromStatus,
  startLabel,
  startingSession,
} from "./game";

function status(state: GameRunStatus["state"], exitCode: number | null = null): GameRunStatus {
  return { instanceId: "a", state, pid: 4242, exitCode, discardedLogs: 0 };
}

describe("sessionFromStatus", () => {
  it("maps a running process and keeps its pid", () => {
    expect(sessionFromStatus(status("running"))).toEqual({
      phase: "running",
      pid: 4242,
      exitCode: null,
      error: null,
    });
  });

  it("maps stopped and exited, keeping the exit code", () => {
    expect(sessionFromStatus(status("stopped", 1)).phase).toBe("stopped");
    expect(sessionFromStatus(status("exited", 0))).toMatchObject({ phase: "exited", exitCode: 0, pid: null });
  });

  it("turns a failed process into an explained failure", () => {
    expect(sessionFromStatus(status("failed", 1)).error).toContain("退出码 1");
    expect(sessionFromStatus(status("failed", null)).error).toContain("异常");
  });
});

describe("describeSession", () => {
  it("describes every phase", () => {
    expect(describeSession(idleSession)).toEqual({ label: "就绪", tone: "success" });
    expect(describeSession(startingSession).label).toBe("启动中");
    expect(describeSession(sessionFromStatus(status("running"))).label).toBe("运行中");
    expect(describeSession(sessionFromStatus(status("stopped"))).label).toBe("已停止");
    expect(describeSession(failedSession("x")).tone).toBe("danger");
  });

  it("calls an exit with code 0 normal and any other code abnormal", () => {
    expect(describeSession(sessionFromStatus(status("exited", 0)))).toEqual({ label: "已退出", tone: "subtle" });
    expect(describeSession(sessionFromStatus(status("exited", null))).label).toBe("已退出");
    const crashed = describeSession(sessionFromStatus(status("exited", 137)));
    expect(crashed.tone).toBe("danger");
    expect(crashed.label).toContain("137");
  });
});

describe("what the buttons may do", () => {
  it("allows starting when idle, finished or failed but not while starting or running", () => {
    expect(canStart(idleSession)).toBe(true);
    expect(canStart(failedSession("x"))).toBe(true);
    expect(canStart(sessionFromStatus(status("exited", 0)))).toBe(true);
    expect(canStart(startingSession)).toBe(false);
    expect(canStart(sessionFromStatus(status("running")))).toBe(false);
  });

  it("allows stopping only while running", () => {
    expect(canStop(sessionFromStatus(status("running")))).toBe(true);
    expect(canStop(idleSession)).toBe(false);
    expect(canStop(startingSession)).toBe(false);
  });

  it("labels the start button for each situation", () => {
    expect(startLabel(idleSession)).toBe("启动");
    expect(startLabel(startingSession)).toBe("启动中…");
    expect(startLabel(failedSession("x"))).toBe("重试");
  });
});
