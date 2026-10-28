import { describe, expect, it } from "vitest";
import type { LauncherInstallation } from "../ipc";
import { launcherLabel, summariseScan } from "./launcherScan";

function installation(
  kind: LauncherInstallation["kind"],
  versions: string[],
): LauncherInstallation {
  return {
    kind,
    launcherRoot: "C:\\Launchers\\X",
    sources: [
      {
        token: "t",
        sourceKind: kind,
        sourcePath: "C:\\Games\\.minecraft",
        candidates: versions.map((displayName) => ({
          id: displayName,
          displayName,
        })),
      },
    ],
  };
}

describe("summariseScan", () => {
  it("列出启动器数量与去重后的版本数", () => {
    const scan = summariseScan([
      installation("hmcl", ["1.20.1", "1.21.4"]),
      installation("pcl2", ["1.21.4", "1.19.2"]),
    ]);

    expect(scan.installations).toHaveLength(2);
    // 1.21.4 两边都有，只算一次。
    expect(scan.totalVersions).toBe(3);
  });

  it("什么都没找到时是 0，而不是报错", () => {
    expect(summariseScan([]).totalVersions).toBe(0);
  });

  it("启动器有多个来源时把版本合并去重", () => {
    const prism: LauncherInstallation = {
      kind: "prism",
      launcherRoot: "C:\\Prism",
      sources: [
        { token: "a", sourceKind: "prism", sourcePath: "a", candidates: [
          { id: "x", displayName: "1.20.1" },
        ] },
        { token: "b", sourceKind: "prism", sourcePath: "b", candidates: [
          { id: "y", displayName: "1.20.1" },
          { id: "z", displayName: "1.21.4" },
        ] },
      ],
    };

    expect(summariseScan([prism]).totalVersions).toBe(2);
  });
});

describe("launcherLabel", () => {
  it("每种来源都有中文/官方名", () => {
    expect(launcherLabel("pcl2")).toBe("PCL2");
    expect(launcherLabel("hmcl")).toBe("HMCL");
    expect(launcherLabel("bakaxl")).toBe("BakaXL");
    expect(launcherLabel("prism")).toBe("Prism / MultiMC");
    expect(launcherLabel("folder")).toBe("游戏目录");
  });
});
