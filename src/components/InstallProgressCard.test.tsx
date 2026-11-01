import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { InstallProgress } from "../ipc";
import type { InstallJob } from "../state/install";
import { renderWithState } from "../test/support";
import { InstallProgressCard } from "./InstallProgressCard";

function job(overrides: Partial<InstallJob> = {}): InstallJob {
  return {
    version: "1.21.1",
    name: "1.21.1",
    loader: null,
    status: "running",
    progress: null,
    speed: 0,
    instance: null,
    error: null,
    ...overrides,
  };
}

function progress(
  stage: InstallProgress["stage"],
  filesDone = 0,
  filesTotal = 0,
  bytesDone = 0,
  bytesTotal = 0,
): InstallProgress {
  return { version: "1.21.1", stage, filesDone, filesTotal, bytesDone, bytesTotal };
}

/** 步骤按钮的文字，去掉前面那个序号（已完成的那几步是勾）。 */
function labels(): string[] {
  return screen
    .getAllByRole("listitem")
    .map((item) => item.textContent?.replace(/^\d+/, "") ?? "");
}

function step(label: string): HTMLElement {
  const all = screen.getAllByRole("listitem");
  return all[labels().indexOf(label)];
}

describe("InstallProgressCard", () => {
  it("lists the four vanilla steps", () => {
    renderWithState(
      <InstallProgressCard job={job({ progress: progress("assets", 1, 2) })} onCancel={() => undefined} />,
    );

    expect(labels()).toEqual(["获取版本清单", "下载游戏文件", "下载游戏资源", "创建实例"]);
    expect(screen.queryByText("安装模组加载器")).not.toBeInTheDocument();
  });

  it("marks the current vanilla step", () => {
    renderWithState(
      <InstallProgressCard job={job({ progress: progress("assets", 1, 2) })} onCancel={() => undefined} />,
    );

    expect(step("下载游戏资源")).toHaveAttribute("aria-current", "step");
    expect(step("下载游戏资源")).toHaveClass("current");
    expect(step("获取版本清单")).toHaveClass("done");
    expect(step("创建实例")).toHaveClass("pending");
  });

  it("adds the mod loader step while a loader installs", () => {
    renderWithState(
      <InstallProgressCard
        job={job({
          name: "1.21.1-Fabric 0.16.9",
          loader: { kind: "fabric", version: "0.16.9" },
          progress: progress("modLoader", 3, 7),
        })}
        onCancel={() => undefined}
      />,
    );

    // 回归：加载器这一阶段以前不在步骤表里，当前阶段找不到，四个步骤一起退回「等待」。
    expect(labels()).toEqual([
      "获取版本清单",
      "下载游戏文件",
      "下载游戏资源",
      "安装模组加载器",
      "创建实例",
    ]);
    expect(step("安装模组加载器")).toHaveAttribute("aria-current", "step");
    expect(step("下载游戏资源")).toHaveClass("done");
    expect(step("创建实例")).toHaveClass("pending");
    // 加载器按步骤计数：3/7 落在 0.97–0.99 之间。
    expect(screen.getByText("98%")).toBeInTheDocument();
    expect(screen.getByText("文件 3 / 7")).toBeInTheDocument();
  });

  it("keeps the loader step after the loader stage has passed", () => {
    renderWithState(
      <InstallProgressCard
        job={job({
          name: "1.21.1-Fabric 0.16.9",
          loader: { kind: "fabric", version: "0.16.9" },
          progress: progress("finalize", 0, 1),
        })}
        onCancel={() => undefined}
      />,
    );

    expect(labels()).toHaveLength(5);
    expect(step("创建实例")).toHaveAttribute("aria-current", "step");
    expect(step("安装模组加载器")).toHaveClass("done");
  });

  it("follows a mod loader stage even when the job does not know the loader", () => {
    // 后端的阶段是权威的：job 里没记下加载器时也不能丢掉当前步骤。
    renderWithState(
      <InstallProgressCard job={job({ progress: progress("modLoader", 1, 7) })} onCancel={() => undefined} />,
    );

    expect(labels()).toContain("安装模组加载器");
    expect(step("安装模组加载器")).toHaveAttribute("aria-current", "step");
  });

  it("shows the byte counters of a download stage", () => {
    renderWithState(
      <InstallProgressCard
        job={job({ progress: progress("files", 2, 10, 512, 2048), speed: 1024 })}
        onCancel={() => undefined}
      />,
    );

    expect(screen.getByText("512 B / 2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("文件 2 / 10")).toBeInTheDocument();
    expect(screen.getByText("1.0 KB/s · 约 2 秒")).toBeInTheDocument();
  });
});
