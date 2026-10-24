import { describe, expect, it } from "vitest";
import type { JavaRuntimeInfo } from "../ipc";
import {
  dedupeRuntimes,
  describeEmptyRuntimes,
  describeRuntime,
  isDefaultRuntime,
  pickSelectedRuntime,
  runtimeKey,
  shortenPath,
  sortRuntimes,
  vendorLabel,
} from "./javaRuntime";

function runtime(overrides: Partial<JavaRuntimeInfo> = {}): JavaRuntimeInfo {
  return {
    executable: "C:\\Program Files\\Java\\jdk-21\\bin\\java.exe",
    majorVersion: 21,
    architecture: "x64",
    vendor: "Adoptium",
    ...overrides,
  };
}

describe("runtimeKey", () => {
  it("normalises separators and case", () => {
    expect(runtimeKey(runtime({ executable: "C:\\Java\\bin\\java.exe" }))).toBe(
      runtimeKey(runtime({ executable: "c:/java/bin/java.exe" })),
    );
  });
});

describe("dedupeRuntimes", () => {
  it("collapses the same executable found twice", () => {
    const unique = dedupeRuntimes([
      runtime({ executable: "C:\\Java\\bin\\java.exe" }),
      runtime({ executable: "c:/java/bin/java.exe" }),
      runtime({ executable: "C:\\Other\\bin\\java.exe" }),
    ]);

    expect(unique).toHaveLength(2);
  });

  it("keeps distinct installations", () => {
    const unique = dedupeRuntimes([
      runtime({ executable: "C:\\a\\java.exe" }),
      runtime({ executable: "C:\\b\\java.exe" }),
    ]);

    expect(unique.map((item) => item.executable)).toEqual([
      "C:\\a\\java.exe",
      "C:\\b\\java.exe",
    ]);
  });

  it("handles an empty list", () => {
    expect(dedupeRuntimes([])).toEqual([]);
  });
});

describe("sortRuntimes", () => {
  it("puts the newest major version first", () => {
    const sorted = sortRuntimes([
      runtime({ executable: "a", majorVersion: 8 }),
      runtime({ executable: "b", majorVersion: 21 }),
      runtime({ executable: "c", majorVersion: 17 }),
    ]);

    expect(sorted.map((item) => item.majorVersion)).toEqual([21, 17, 8]);
  });

  it("breaks ties by vendor", () => {
    const sorted = sortRuntimes([
      runtime({ executable: "a", vendor: "Zulu" }),
      runtime({ executable: "b", vendor: "Adoptium" }),
    ]);

    expect(sorted.map((item) => item.vendor)).toEqual(["Adoptium", "Zulu"]);
  });

  it("does not mutate the input", () => {
    const input = [
      runtime({ executable: "a", majorVersion: 8 }),
      runtime({ executable: "b", majorVersion: 21 }),
    ];
    sortRuntimes(input);
    expect(input.map((item) => item.majorVersion)).toEqual([8, 21]);
  });
});

describe("describeRuntime", () => {
  it("shows version and architecture", () => {
    expect(describeRuntime(runtime())).toBe("Java 21 · x64");
  });
});

describe("vendorLabel", () => {
  it("falls back when the vendor is missing", () => {
    expect(vendorLabel(runtime({ vendor: null }))).toBe("未知厂商");
    expect(vendorLabel(runtime({ vendor: "  " }))).toBe("未知厂商");
  });

  it("keeps a known vendor", () => {
    expect(vendorLabel(runtime())).toBe("Adoptium");
  });
});

describe("shortenPath", () => {
  it("keeps the last two segments of a Windows path", () => {
    expect(
      shortenPath("C:\\Program Files\\Java\\jdk-21\\bin\\java.exe"),
    ).toBe("…\\bin\\java.exe");
  });

  it("keeps short paths intact", () => {
    expect(shortenPath("java")).toBe("java");
    expect(shortenPath("bin/java")).toBe("bin/java");
    expect(shortenPath("bin\\java")).toBe("bin\\java");
  });

  it("handles POSIX paths", () => {
    expect(shortenPath("/usr/lib/jvm/java-17/bin/java")).toBe("…/bin/java");
  });

  it("handles Windows paths", () => {
    expect(shortenPath("C:\\Program Files\\Java\\jdk-21\\bin\\java.exe")).toBe(
      "…\\bin\\java.exe",
    );
  });
});

describe("pickSelectedRuntime", () => {
  const runtimes = [
    runtime({ executable: "C:\\a\\java.exe", majorVersion: 21 }),
    runtime({ executable: "C:\\b\\java.exe", majorVersion: 17 }),
  ];

  it("returns null for an empty list", () => {
    expect(pickSelectedRuntime([], "c:/a/java.exe")).toBeNull();
  });

  it("honours a saved key that still exists", () => {
    expect(pickSelectedRuntime(runtimes, "c:/b/java.exe")?.majorVersion).toBe(17);
  });

  it("falls back to the first runtime when the saved key is gone", () => {
    expect(pickSelectedRuntime(runtimes, "c:/gone/java.exe")?.majorVersion).toBe(
      21,
    );
  });

  it("defaults to the first runtime when nothing is saved", () => {
    expect(pickSelectedRuntime(runtimes, null)?.majorVersion).toBe(21);
  });
});

describe("isDefaultRuntime", () => {
  const runtimes = [
    runtime({ executable: "C:\\a\\java.exe", majorVersion: 21 }),
    runtime({ executable: "C:\\b\\java.exe", majorVersion: 17 }),
  ];

  it("treats the first runtime as default when nothing is saved", () => {
    expect(isDefaultRuntime(runtimes[0], runtimes, null)).toBe(true);
    expect(isDefaultRuntime(runtimes[1], runtimes, null)).toBe(false);
  });

  it("follows the saved choice", () => {
    expect(isDefaultRuntime(runtimes[1], runtimes, "c:/b/java.exe")).toBe(true);
    expect(isDefaultRuntime(runtimes[0], runtimes, "c:/b/java.exe")).toBe(false);
  });

  it("is false when there are no runtimes", () => {
    expect(isDefaultRuntime(runtimes[0], [], null)).toBe(false);
  });
});

describe("describeEmptyRuntimes", () => {
  it("points at the desktop build outside Tauri", () => {
    expect(describeEmptyRuntimes(false)).toContain("桌面版");
  });

  it("gives installation advice on the desktop", () => {
    expect(describeEmptyRuntimes(true)).toContain("暂未检测到 Java");
  });
});
