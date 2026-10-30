import { describe, expect, it } from "vitest";
import type { InstallProgress, InstallStage } from "../ipc";
import {
  describeStage,
  emptySpeedSample,
  formatBytes,
  formatRemaining,
  formatSpeed,
  installStages,
  nextSpeed,
  overallFraction,
  stageFraction,
} from "./install";

function progress(
  stage: InstallStage,
  bytesDone: number,
  bytesTotal: number,
  filesDone = 0,
  filesTotal = 0,
): InstallProgress {
  return { version: "1.21.1", stage, bytesDone, bytesTotal, filesDone, filesTotal };
}

describe("overallFraction", () => {
  it("is zero before any progress arrives", () => {
    expect(overallFraction(null)).toBe(0);
  });

  it("only moves forward across the four stages", () => {
    const samples = [
      progress("metadata", 0, 0),
      progress("files", 0, 100),
      progress("files", 100, 100),
      progress("assets", 0, 100),
      progress("assets", 50, 100),
      progress("assets", 100, 100),
      progress("finalize", 0, 0),
    ].map(overallFraction);
    for (let index = 1; index < samples.length; index += 1) {
      expect(samples[index]).toBeGreaterThanOrEqual(samples[index - 1]);
    }
    expect(samples[samples.length - 1]).toBeLessThanOrEqual(1);
    expect(samples[0]).toBe(0);
  });

  it("keeps moving forward through a mod loader install, which has an extra stage", () => {
    // 加载器按步骤计数：files_done / files_total，没有字节数。
    const samples = [
      progress("metadata", 0, 0),
      progress("assets", 100, 100),
      progress("modLoader", 0, 7),
      progress("modLoader", 3, 7),
      progress("modLoader", 7, 7),
      progress("finalize", 0, 1),
    ].map(overallFraction);
    for (let index = 1; index < samples.length; index += 1) {
      expect(samples[index]).toBeGreaterThanOrEqual(samples[index - 1]);
    }
    expect(samples.every((value) => Number.isFinite(value) && value <= 1)).toBe(true);
  });

  it("does not throw on the modLoader stage the backend sends", () => {
    // 回归：这一阶段以前不在表里，渲染进度条时会 `undefined is not iterable`。
    expect(() => overallFraction(progress("modLoader", 1, 7))).not.toThrow();
    expect(describeStage("modLoader")).toBe("安装模组加载器");
  });

  it("falls back to file counts when no byte total is known", () => {
    expect(overallFraction(progress("assets", 0, 0, 5, 10))).toBeGreaterThan(
      overallFraction(progress("assets", 0, 0, 0, 10)),
    );
  });

  it("never exceeds the end of its stage when the counters overshoot", () => {
    expect(overallFraction(progress("files", 500, 100))).toBeCloseTo(0.3);
  });
});

describe("stageFraction", () => {
  it("is the share of bytes done, clamped to 0–1", () => {
    expect(stageFraction(null)).toBe(0);
    expect(stageFraction(progress("files", 25, 100))).toBe(0.25);
    expect(stageFraction(progress("files", 500, 100))).toBe(1);
    expect(stageFraction(progress("assets", 0, 0, 3, 4))).toBe(0.75);
    expect(stageFraction(progress("metadata", 0, 0))).toBe(0);
  });
});

describe("nextSpeed", () => {
  it("starts a fresh sample on the first progress and on a new stage", () => {
    const first = nextSpeed(emptySpeedSample, progress("files", 1000, 5000), 10_000);
    expect(first).toEqual({ stage: "files", at: 10_000, bytes: 1000, speed: 0 });

    const next = nextSpeed(first, progress("assets", 0, 5000), 11_000);
    expect(next.stage).toBe("assets");
    expect(next.speed).toBe(0);
  });

  it("measures bytes per second and smooths later samples", () => {
    const start = nextSpeed(emptySpeedSample, progress("files", 0, 10_000), 0);
    const one = nextSpeed(start, progress("files", 1000, 10_000), 1000);
    expect(one.speed).toBeCloseTo(1000);

    const two = nextSpeed(one, progress("files", 4000, 10_000), 2000);
    // 3000 B/s instantaneous, blended with the previous 1000 B/s
    expect(two.speed).toBeGreaterThan(1000);
    expect(two.speed).toBeLessThan(3000);
  });

  it("ignores samples that arrive too quickly", () => {
    const start = nextSpeed(emptySpeedSample, progress("files", 0, 10_000), 0);
    expect(nextSpeed(start, progress("files", 10, 10_000), 100)).toBe(start);
  });

  it("restarts when the byte count goes backwards after a retry", () => {
    const start = nextSpeed(emptySpeedSample, progress("files", 5000, 10_000), 0);
    const retry = nextSpeed(start, progress("files", 100, 10_000), 1000);
    expect(retry.speed).toBe(0);
    expect(retry.bytes).toBe(100);
  });
});

describe("formatting", () => {
  it("formats byte counts with sensible precision", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(250 * 1024 * 1024)).toBe("250 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
    expect(formatBytes(-5)).toBe("0 B");
  });

  it("shows a dash when the speed is unknown", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(2 * 1024 * 1024)).toBe("2.0 MB/s");
  });

  it("estimates the remaining time", () => {
    expect(formatRemaining(1000, 0)).toBe("");
    expect(formatRemaining(0, 100)).toBe("");
    expect(formatRemaining(3000, 100)).toBe("约 30 秒");
    expect(formatRemaining(12_500, 100)).toBe("约 2 分 5 秒");
    expect(formatRemaining(400_000, 100)).toBe("约 1 小时 6 分");
  });

  it("labels every stage and lists them in order", () => {
    expect(installStages).toEqual(["metadata", "files", "assets", "finalize"]);
    for (const stage of installStages) {
      expect(describeStage(stage).length).toBeGreaterThan(0);
    }
  });
});
