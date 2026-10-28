import { describe, expect, it } from "vitest";
import type { InstanceSummary } from "../ipc";
import {
  coverHue,
  describeInstanceState,
  describeSkippedReason,
  filterInstances,
  sortInstances,
  summariseInstances,
  validateInstanceName,
} from "./instances";

function instance(overrides: Partial<InstanceSummary> = {}): InstanceSummary {
  return {
    id: "id",
    name: "Survival",
    version: "1.21.1",
    state: "ready",
    ...overrides,
  };
}

describe("describeInstanceState", () => {
  it.each([
    ["ready", "就绪", "success"],
    ["incomplete", "待补齐", "warning"],
    ["running", "运行中", "informative"],
    ["failed", "失败", "danger"],
  ] as const)("maps %s to a %s badge with the %s tone", (state, label, tone) => {
    const view = describeInstanceState(state);
    expect(view.label).toBe(label);
    expect(view.tone).toBe(tone);
    expect(view.hint.length).toBeGreaterThan(0);
  });

  it("gives every state a distinct label", () => {
    const labels = (["ready", "incomplete", "running", "failed"] as const).map(
      (state) => describeInstanceState(state).label,
    );
    expect(new Set(labels).size).toBe(labels.length);
  });
});

describe("sortInstances", () => {
  it("puts running instances first", () => {
    const sorted = sortInstances([
      instance({ id: "a", name: "Alpha", state: "ready" }),
      instance({ id: "b", name: "Beta", state: "running" }),
      instance({ id: "c", name: "Gamma", state: "failed" }),
    ]);

    expect(sorted.map((item) => item.id)).toEqual(["b", "a", "c"]);
  });

  it("orders by name within the same state", () => {
    const sorted = sortInstances([
      instance({ id: "c", name: "Charlie" }),
      instance({ id: "a", name: "alpha" }),
      instance({ id: "b", name: "Bravo" }),
    ]);

    expect(sorted.map((item) => item.name)).toEqual([
      "alpha",
      "Bravo",
      "Charlie",
    ]);
  });

  it("does not mutate the input", () => {
    const input = [
      instance({ id: "b", name: "Beta", state: "failed" }),
      instance({ id: "a", name: "Alpha", state: "running" }),
    ];
    const snapshot = input.map((item) => item.id);

    sortInstances(input);

    expect(input.map((item) => item.id)).toEqual(snapshot);
  });

  it("handles an empty list", () => {
    expect(sortInstances([])).toEqual([]);
  });
});

describe("filterInstances", () => {
  const list = [
    instance({ id: "a", name: "Survival", version: "1.21.1" }),
    instance({ id: "b", name: "创造模式", version: "1.20.1" }),
    instance({ id: "c", name: "Redstone Lab", version: "1.21.1" }),
  ];

  it("returns everything for a blank query", () => {
    expect(filterInstances(list, "   ")).toHaveLength(3);
  });

  it("matches the name case-insensitively", () => {
    expect(filterInstances(list, "surv").map((item) => item.id)).toEqual(["a"]);
    expect(filterInstances(list, "REDSTONE").map((item) => item.id)).toEqual([
      "c",
    ]);
  });

  it("matches the version", () => {
    expect(filterInstances(list, "1.20").map((item) => item.id)).toEqual(["b"]);
  });

  it("matches non-latin names", () => {
    expect(filterInstances(list, "创造").map((item) => item.id)).toEqual(["b"]);
  });

  it("returns an empty list when nothing matches", () => {
    expect(filterInstances(list, "forge")).toEqual([]);
  });

  it("does not mutate the input", () => {
    const input = [...list];
    filterInstances(input, "surv");
    expect(input).toHaveLength(3);
  });
});

describe("summariseInstances", () => {
  it("says so when the list is empty", () => {
    expect(summariseInstances([])).toBe("还没有实例");
  });

  it("counts instances", () => {
    expect(summariseInstances([instance(), instance({ id: "b" })])).toBe(
      "共 2 个实例",
    );
  });

  it("mentions running instances", () => {
    expect(
      summariseInstances([
        instance({ id: "a", state: "running" }),
        instance({ id: "b" }),
      ]),
    ).toBe("2 个实例，1 个运行中");
  });
});

describe("describeSkippedReason", () => {
  it("translates the known reasons", () => {
    expect(describeSkippedReason("unreadable")).toBe("文件无法读取");
    expect(describeSkippedReason("invalid-json")).toBe("实例文件格式损坏");
    expect(describeSkippedReason("unsupported-version")).toBe(
      "版本标识不受支持",
    );
  });

  it("passes unknown reasons through", () => {
    expect(describeSkippedReason("disk-full")).toBe("disk-full");
  });
});

describe("validateInstanceName", () => {
  it("accepts a normal name", () => {
    expect(validateInstanceName("生存 1.21")).toBeNull();
  });

  it("rejects an empty or blank name", () => {
    expect(validateInstanceName("")).not.toBeNull();
    expect(validateInstanceName("   ")).not.toBeNull();
  });

  it("rejects names longer than 64 characters", () => {
    expect(validateInstanceName("a".repeat(64))).toBeNull();
    expect(validateInstanceName("a".repeat(65))).not.toBeNull();
  });
});

describe("coverHue", () => {
  it("is stable for one version and inside the hue range", () => {
    expect(coverHue("1.21.1")).toBe(coverHue("1.21.1"));
    for (const version of ["1.21.1", "24w14a", "1.8.9", "a"]) {
      const hue = coverHue(version);
      expect(hue).toBeGreaterThanOrEqual(0);
      expect(hue).toBeLessThan(360);
    }
  });

  it("separates neighbouring versions", () => {
    expect(coverHue("1.21")).not.toBe(coverHue("1.21.1"));
    expect(coverHue("1.20.1")).not.toBe(coverHue("1.20.4"));
  });
});
