import type { GameRunStatus, InstanceState } from "../ipc";

/**
 * 启动面板的纯逻辑：把「运行状态 + 忙态 + 激活错误」收敛成一个阶段，
 * 再由阶段推导徽章、按钮文案和可用性。组件只负责组装与调 IPC。
 *
 * 注意 `GameRunStatus` 是 activate/start/stop 命令的一次性返回值，
 * 不是事件流——命令返回后前端不会收到进程退出等后续通知。
 */

export type LaunchPhase =
  | "installed-ready"
  | "missing-files"
  | "launching"
  | "running"
  | "stopped"
  | "failed";

/**
 * 推导当前阶段。
 *
 * 忙态优先（正在启动时别的一切都让路）；其次是激活失败——文件没补齐
 * 之前谈不上启动；最后才看后端报告的运行状态，没有状态就是待启动。
 */
export function launchPhase(
  status: GameRunStatus | null,
  busy: boolean,
  activateError: string | null,
): LaunchPhase {
  if (busy) {
    return "launching";
  }
  if (activateError !== null) {
    return "missing-files";
  }
  if (status === null) {
    return "installed-ready";
  }
  switch (status.state) {
    case "running":
      return "running";
    case "failed":
      return "failed";
    case "exited":
    case "stopped":
      return "stopped";
  }
}

export type PhaseView = {
  label: string;
  tone: "informative" | "success" | "warning" | "danger";
};

const phaseViews: Record<LaunchPhase, PhaseView> = {
  "installed-ready": { label: "可启动", tone: "success" },
  "missing-files": { label: "缺文件", tone: "warning" },
  launching: { label: "启动中", tone: "informative" },
  running: { label: "运行中", tone: "informative" },
  stopped: { label: "已停止", tone: "informative" },
  failed: { label: "失败", tone: "danger" },
};

export function describePhase(phase: LaunchPhase): PhaseView {
  return phaseViews[phase];
}

/**
 * 面板挂载时的初始标签：`InstanceSummary.state` 来自实例清单，只说明
 * 「上次已知」的安装情况，不是新鲜进程状态——reload 后不能假装知道
 * 进程还活着，所以措辞一律带「未验证」。
 */
export function initialLabel(state: InstanceState): PhaseView {
  switch (state) {
    case "ready":
      return { label: "未验证", tone: "informative" };
    case "running":
      return { label: "运行中（未验证）", tone: "informative" };
    case "incomplete":
      return { label: "未验证（缺文件）", tone: "warning" };
    case "failed":
      return { label: "未验证（上次失败）", tone: "danger" };
  }
}

/** 启动按钮什么时候可点：待启动、已停止、失败或缺文件（重试=重新激活）。 */
export function canStart(phase: LaunchPhase): boolean {
  return (
    phase === "installed-ready" ||
    phase === "missing-files" ||
    phase === "stopped" ||
    phase === "failed"
  );
}

/** 停止按钮只在游戏进程真的在跑时可用。 */
export function canStop(phase: LaunchPhase): boolean {
  return phase === "running";
}

export function buttonLabel(phase: LaunchPhase): string {
  switch (phase) {
    case "installed-ready":
      return "启动";
    case "missing-files":
      return "重试";
    case "launching":
      return "启动中…";
    case "running":
      return "停止";
    case "stopped":
      return "启动";
    case "failed":
      return "重试";
  }
}

/**
 * 退出码的可读文案。没有状态或后端没给退出码（被外部杀掉等）时
 * 返回空字符串，调用方据此不渲染这一段。
 */
export function describeExit(status: GameRunStatus | null): string {
  if (status === null || status.exitCode === null) {
    return "";
  }
  return `退出码 ${status.exitCode}`;
}
