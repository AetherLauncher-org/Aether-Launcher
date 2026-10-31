import { describe, expect, it } from "vitest";
import { createT, formatMessage, translate } from "./messages";
import type { MessageCatalog } from "./messages";

const zh = {
  "hello": "你好",
  "greet": "你好，{name}",
  "install": "安装 {version} · {percent}%",
  "fileCount.one": "{count} 个文件",
  "fileCount.other": "{count} 个文件",
} satisfies MessageCatalog;

const en = {
  "hello": "Hello",
  "greet": "Hello, {name}",
  "fileCount.one": "{count} file",
  "fileCount.other": "{count} files",
} satisfies MessageCatalog;

const catalogs = { "zh-CN": zh, "en-US": en };

describe("formatMessage", () => {
  it("replaces known placeholders", () => {
    expect(formatMessage("你好，{name}", { name: "Steve" })).toBe("你好，Steve");
  });

  it("leaves missing placeholders visible", () => {
    expect(formatMessage("你好，{name}")).toBe("你好，{name}");
  });

  it("leaves unknown params alone", () => {
    expect(formatMessage("你好，{name}", { other: "x" })).toBe("你好，{name}");
  });

  it("coerces numbers to strings", () => {
    expect(formatMessage("{n}", { n: 42 })).toBe("42");
  });

  it("returns the template untouched without params", () => {
    expect(formatMessage("你好")).toBe("你好");
  });
});

describe("translate", () => {
  it("resolves a key in the current locale", () => {
    expect(translate(catalogs, "zh-CN", "en-US", "hello")).toBe("Hello");
  });

  it("falls back to zh-CN for a missing key", () => {
    // "install" 只在 zh-CN 里
    expect(translate(catalogs, "zh-CN", "en-US", "install", { version: "1.21.1", percent: 17 })).toBe(
      "安装 1.21.1 · 17%",
    );
  });

  it("returns the key itself when it exists nowhere", () => {
    expect(translate(catalogs, "zh-CN", "en-US", "nope.missing")).toBe("nope.missing");
  });

  it("uses plural categories via count", () => {
    expect(translate(catalogs, "zh-CN", "en-US", "fileCount", { count: 1 })).toBe("1 file");
    expect(translate(catalogs, "zh-CN", "en-US", "fileCount", { count: 5 })).toBe("5 files");
  });

  it("falls back to the .other category when a category is absent", () => {
    // zh-CN 只有一个 other 类别，任何数量都落 other
    expect(translate(catalogs, "zh-CN", "zh-CN", "fileCount", { count: 1 })).toBe("1 个文件");
  });
});

describe("createT", () => {
  it("binds a catalog and locale into a plain function", () => {
    const t = createT(catalogs, "zh-CN", "en-US");
    expect(t("greet", { name: "Alex" })).toBe("Hello, Alex");
  });
});
