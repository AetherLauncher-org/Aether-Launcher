import type { InstanceState, InstanceSummary } from "../ipc";

/**
 * 实例页的纯逻辑：排序、过滤、状态到徽章的映射、创建对话框的校验。
 *
 * 页面只负责组装，所有判断都在这里，方便直接单测。
 */

/** 徽章的语义色。与 Fluent 的 `Badge` 的 `color` 取值一致。 */
export type BadgeTone =
  | "success"
  | "warning"
  | "informative"
  | "danger"
  | "subtle";

export type InstanceStatusView = {
  /** 界面上显示的标签 */
  label: string;
  tone: BadgeTone;
  /** 说明为什么会是这个状态，用于 tooltip */
  hint: string;
};

const statusViews: Record<InstanceState, InstanceStatusView> = {
  ready: {
    label: "就绪",
    tone: "success",
    hint: "文件齐全，可以直接启动",
  },
  incomplete: {
    label: "待补齐",
    tone: "warning",
    hint: "缺少版本文件或资源，启动前需要补齐",
  },
  running: {
    label: "运行中",
    tone: "informative",
    hint: "游戏进程正在运行",
  },
  failed: {
    label: "失败",
    tone: "danger",
    hint: "上次启动失败，可以查看原因后重试",
  },
};

export function describeInstanceState(state: InstanceState): InstanceStatusView {
  return statusViews[state];
}

/** 排序：运行中优先，其次按状态，同状态按名称（中文按拼音）。 */
const statusRank: Record<InstanceState, number> = {
  running: 0,
  ready: 1,
  incomplete: 2,
  failed: 3,
};

const collator = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});

export function sortInstances(
  instances: readonly InstanceSummary[],
): InstanceSummary[] {
  return [...instances].sort((left, right) => {
    const byStatus = statusRank[left.state] - statusRank[right.state];
    if (byStatus !== 0) {
      return byStatus;
    }
    return collator.compare(left.name, right.name);
  });
}

/** 名称或版本里包含关键词（不区分大小写）就保留。空关键词返回全部。 */
export function filterInstances(
  instances: readonly InstanceSummary[],
  query: string,
): InstanceSummary[] {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return [...instances];
  }
  return instances.filter(
    (instance) =>
      instance.name.toLowerCase().includes(needle) ||
      instance.version.toLowerCase().includes(needle),
  );
}

/** 页面上一句话概括列表。 */
export function summariseInstances(
  instances: readonly InstanceSummary[],
): string {
  if (instances.length === 0) {
    return "还没有实例";
  }
  const running = instances.filter((item) => item.state === "running").length;
  if (running > 0) {
    return `${instances.length} 个实例，${running} 个运行中`;
  }
  return `共 ${instances.length} 个实例`;
}

/**
 * 把后端给的跳过原因翻译成人话。后端只保证是稳定的短分类，
 * 所以未知值原样显示，方便对照日志。
 */
export function describeSkippedReason(reason: string): string {
  switch (reason) {
    case "unreadable":
      return "文件无法读取";
    case "invalid-json":
      return "实例文件格式损坏";
    case "unsupported-version":
      return "版本标识不受支持";
    default:
      return reason;
  }
}

/** 名称校验：去掉首尾空格后不能为空，长度不超过 64。 */
export function validateInstanceName(raw: string): string | null {
  const name = raw.trim();
  if (name.length === 0) {
    return "请填写实例名称";
  }
  if (name.length > 64) {
    return "实例名称不能超过 64 个字符";
  }
  return null;
}

/**
 * 封面色相（0–359）。由版本号的字符累加得到，同一个版本永远得到同一个值，
 * 乘一个质数让相邻版本（1.21 与 1.21.1）的颜色拉开距离。
 */
export function coverHue(version: string): number {
  let hash = 7;
  for (const character of version) {
    hash = (hash * 31 + character.codePointAt(0)!) % 360;
  }
  return (hash * 47) % 360;
}
