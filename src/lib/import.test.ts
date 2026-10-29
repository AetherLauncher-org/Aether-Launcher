import { describe, expect, it } from "vitest";
import {
  allCandidateIds,
  describeImportSkipReason,
  describeImportSourceKind,
  summariseImport,
  toggleAllCandidates,
  toggleCandidate,
} from "./import";
import type { ImportSource } from "../ipc";

const source: ImportSource = {
  token: "t",
  sourceKind: "folder",
  sourcePath: "D:\\MC",
  candidates: [
    { id: "1.20.1", displayName: "1.20.1" },
    { id: "1.21", displayName: "1.21" },
  ],
};

describe("describeImportSourceKind", () => {
  it("labels the known launchers", () => {
    expect(describeImportSourceKind("pcl2")).toBe("PCL2");
    expect(describeImportSourceKind("hmcl")).toBe("HMCL");
    expect(describeImportSourceKind("bakaxl")).toBe("BakaXL");
    expect(describeImportSourceKind("prism")).toBe("Prism / MultiMC");
    expect(describeImportSourceKind("folder")).toBe("普通游戏文件夹");
  });

  it("passes an unknown label through", () => {
    expect(describeImportSourceKind("future")).toBe("future");
  });
});

describe("describeImportSkipReason", () => {
  it("translates every reason the backend can send", () => {
    const reasons = [
      "unsafe-version-id",
      "missing-version-json",
      "missing-pack-metadata",
      "unsafe-source",
      "version-json-too-large",
      "invalid-version-json",
      "needs-loader",
      "not-official",
      "duplicate-name",
      "invalid-name",
      "install-failed",
      "cancelled",
    ];

    for (const reason of reasons) {
      const label = describeImportSkipReason(reason);
      expect(label).not.toBe(reason);
      expect(label.length).toBeGreaterThan(0);
    }
  });

  it("passes an unknown reason through", () => {
    expect(describeImportSkipReason("future-reason")).toBe("future-reason");
  });
});

describe("selection helpers", () => {
  it("lists every candidate of a source", () => {
    expect(allCandidateIds(source)).toEqual(["1.20.1", "1.21"]);
  });

  it("toggles one candidate without mutating the input", () => {
    const selected = new Set(["1.20.1"]);
    const next = toggleCandidate(selected, "1.21");

    expect([...next]).toEqual(["1.20.1", "1.21"]);
    expect([...selected]).toEqual(["1.20.1"]);
    expect([...toggleCandidate(next, "1.20.1")]).toEqual(["1.21"]);
  });

  it("selects everything, then clears when everything is already selected", () => {
    const ids = allCandidateIds(source);

    const all = toggleAllCandidates(new Set(), ids);
    expect([...all]).toEqual(ids);
    expect([...toggleAllCandidates(all, ids)]).toEqual([]);
  });

  it("selects everything when the selection is partial", () => {
    const ids = allCandidateIds(source);
    expect([...toggleAllCandidates(new Set(["1.21"]), ids)]).toEqual(ids);
  });
});

describe("summariseImport", () => {
  const instance = {
    id: "a",
    name: "1.20.1",
    version: "1.20.1",
    state: "incomplete" as const,
    category: "vanilla" as const,
    gameDirectory: null,
  };

  it("reports a clean import", () => {
    expect(summariseImport({ imported: [instance], skipped: [] })).toBe(
      "已导入 1 个实例",
    );
  });

  it("reports a partial import", () => {
    expect(
      summariseImport({
        imported: [instance],
        skipped: [{ id: "1.21", reason: "duplicate-name" }],
      }),
    ).toBe("已导入 1 个实例，1 个版本被跳过");
  });

  it("reports a fully skipped import", () => {
    expect(
      summariseImport({
        imported: [],
        skipped: [{ id: "1.21", reason: "missing-version-json" }],
      }),
    ).toBe("没有导入成功，1 个版本被跳过");
  });

  it("reports an empty result", () => {
    expect(summariseImport({ imported: [], skipped: [] })).toBe(
      "没有导入任何版本",
    );
  });
});
