import { describe, expect, it } from "vitest";
import type { GameRunStatus } from "../ipc";
import type { LaunchPhase } from "./launchState";
import {
  buttonLabel,
  canStart,
  canStop,
  describeExit,
  describePhase,
  initialLabel,
  launchPhase,
} from "./launchState";

function status(overrides: Partial<GameRunStatus> = {}): GameRunStatus {
  return {
    instanceId: "a",
    state: "exited",
    pid: 0,
    exitCode: 0,
    discardedLogs: 0,
    ...overrides,
  };
}

describe("launchPhase", () => {
  it("defaults to installed-ready without a run status", () => {
    expect(launchPhase(null, false, null)).toBe("installed-ready");
  });

  it("prefers launching whenever the panel is busy", () => {
    expect(launchPhase(null, true, null)).toBe("launching");
    expect(
      launchPhase(status({ state: "running", pid: 42 }), true, null),
    ).toBe("launching");
    expect(launchPhase(null, true, "缺文件")).toBe("launching");
  });

  it("treats an activation error as missing files", () => {
    expect(launchPhase(null, false, "版本文件缺失")).toBe("missing-files");
    expect(launchPhase(status({ state: "running" }), false, "缺文件")).toBe(
      "missing-files",
    );
  });

  it.each([
    ["running", "running"],
    ["failed", "failed"],
    ["exited", "stopped"],
    ["stopped", "stopped"],
  ] as const)("maps state %s to phase %s", (state, phase) => {
    expect(launchPhase(status({ state }), false, null)).toBe(phase);
  });
});

describe("describePhase", () => {
  it.each([
    ["installed-ready", "可启动", "success"],
    ["missing-files", "缺文件", "warning"],
    ["launching", "启动中", "informative"],
    ["running", "运行中", "informative"],
    ["stopped", "已停止", "informative"],
    ["failed", "失败", "danger"],
  ] as const)("maps %s to the label %s with tone %s", (phase, label, tone) => {
    const view = describePhase(phase as LaunchPhase);
    expect(view.label).toBe(label);
    expect(view.tone).toBe(tone);
  });

  it("gives every phase a distinct label", () => {
    const phases: LaunchPhase[] = [
      "installed-ready",
      "missing-files",
      "launching",
      "running",
      "stopped",
      "failed",
    ];
    const labels = phases.map((phase) => describePhase(phase).label);
    expect(new Set(labels).size).toBe(labels.length);
  });
});

describe("initialLabel", () => {
  it.each([
    ["ready", "未验证", "informative"],
    ["running", "运行中（未验证）", "informative"],
    ["incomplete", "未验证（缺文件）", "warning"],
    ["failed", "未验证（上次失败）", "danger"],
  ] as const)(
    "maps initial state %s to the label %s with tone %s",
    (state, label, tone) => {
      const view = initialLabel(state);
      expect(view.label).toBe(label);
      expect(view.tone).toBe(tone);
    },
  );
});

describe("canStart / canStop", () => {
  it("allows starting from every idle phase", () => {
    expect(canStart("installed-ready")).toBe(true);
    expect(canStart("missing-files")).toBe(true);
    expect(canStart("stopped")).toBe(true);
    expect(canStart("failed")).toBe(true);
  });

  it("blocks starting while launching or running", () => {
    expect(canStart("launching")).toBe(false);
    expect(canStart("running")).toBe(false);
  });

  it("only allows stopping a running game", () => {
    expect(canStop("running")).toBe(true);
    expect(canStop("launching")).toBe(false);
    expect(canStop("installed-ready")).toBe(false);
    expect(canStop("stopped")).toBe(false);
    expect(canStop("failed")).toBe(false);
    expect(canStop("missing-files")).toBe(false);
  });
});

describe("buttonLabel", () => {
  it.each([
    ["installed-ready", "启动"],
    ["stopped", "启动"],
    ["missing-files", "重试"],
    ["failed", "重试"],
    ["launching", "启动中…"],
    ["running", "停止"],
  ] as const)("labels %s as %s", (phase, label) => {
    expect(buttonLabel(phase as LaunchPhase)).toBe(label);
  });
});

describe("describeExit", () => {
  it("returns an empty string without a status or exit code", () => {
    expect(describeExit(null)).toBe("");
    expect(describeExit(status({ exitCode: null }))).toBe("");
  });

  it("formats a numeric exit code", () => {
    expect(describeExit(status({ exitCode: 0 }))).toBe("退出码 0");
    expect(describeExit(status({ state: "failed", exitCode: 1 }))).toBe(
      "退出码 1",
    );
  });
});
