import type { InstallProgress, InstallStage } from "../ipc";

/**
 * 安装进度的纯逻辑：阶段文案、整体进度、速度估算、字节格式化。
 */

export const installStages: readonly InstallStage[] = [
  "metadata",
  "files",
  "assets",
  "finalize",
];

const stageLabels: Record<InstallStage, string> = {
  metadata: "获取版本清单",
  files: "下载游戏文件",
  assets: "下载游戏资源",
  finalize: "创建实例",
};

export function describeStage(stage: InstallStage): string {
  return stageLabels[stage];
}

/** 每个阶段在整条进度条里占的区间（下载资源的字节数最多，所以占得最宽）。 */
const stageRanges: Record<InstallStage, readonly [number, number]> = {
  metadata: [0, 0.04],
  files: [0.04, 0.3],
  assets: [0.3, 0.97],
  finalize: [0.97, 1],
};

/** 0–1 的整体进度。阶段内按字节比例推进，没有总量时停在阶段起点。 */
export function overallFraction(progress: InstallProgress | null): number {
  if (progress === null) {
    return 0;
  }
  const [start, end] = stageRanges[progress.stage];
  const within =
    progress.bytesTotal > 0
      ? Math.min(1, progress.bytesDone / progress.bytesTotal)
      : progress.filesTotal > 0
        ? Math.min(1, progress.filesDone / progress.filesTotal)
        : 0;
  return start + (end - start) * within;
}

/** 当前阶段内的 0–1 进度，用于阶段进度条。 */
export function stageFraction(progress: InstallProgress | null): number {
  if (progress === null) {
    return 0;
  }
  if (progress.bytesTotal > 0) {
    return Math.min(1, progress.bytesDone / progress.bytesTotal);
  }
  return progress.filesTotal > 0 ? Math.min(1, progress.filesDone / progress.filesTotal) : 0;
}

/** 速度估算的采样。`speed` 是指数滑动平均后的字节/秒。 */
export type SpeedSample = {
  stage: InstallStage | null;
  at: number;
  bytes: number;
  speed: number;
};

export const emptySpeedSample: SpeedSample = { stage: null, at: 0, bytes: 0, speed: 0 };

const smoothing = 0.3;

/**
 * 用新的进度更新速度。阶段变化或字节数倒退（重试）时重置采样，
 * 间隔太短的采样直接忽略，避免除以接近 0 的时间。
 */
export function nextSpeed(
  previous: SpeedSample,
  progress: InstallProgress,
  now: number,
): SpeedSample {
  if (previous.stage !== progress.stage || progress.bytesDone < previous.bytes) {
    return { stage: progress.stage, at: now, bytes: progress.bytesDone, speed: 0 };
  }
  const elapsed = (now - previous.at) / 1000;
  if (elapsed < 0.25) {
    return previous;
  }
  const instant = (progress.bytesDone - previous.bytes) / elapsed;
  const speed = previous.speed === 0 ? instant : previous.speed * (1 - smoothing) + instant * smoothing;
  return { stage: progress.stage, at: now, bytes: progress.bytesDone, speed };
}

const units = ["B", "KB", "MB", "GB"] as const;

export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = unit === 0 || value >= 100 ? value.toFixed(0) : value.toFixed(1);
  return `${text} ${units[unit]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return bytesPerSecond > 0 ? `${formatBytes(bytesPerSecond)}/s` : "—";
}

/** 剩余时间的粗略估计，例如 `约 2 分 5 秒`；速度未知时返回空字符串。 */
export function formatRemaining(bytesLeft: number, bytesPerSecond: number): string {
  if (bytesPerSecond <= 0 || bytesLeft <= 0) {
    return "";
  }
  const seconds = Math.ceil(bytesLeft / bytesPerSecond);
  if (seconds < 60) {
    return `约 ${seconds} 秒`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `约 ${minutes} 分 ${seconds % 60} 秒`;
  }
  return `约 ${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分`;
}
