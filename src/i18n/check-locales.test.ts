import { describe, expect, it } from "vitest";
import { checkCatalogs } from "../../scripts/check-locales.mjs";
import { bundledCatalogs } from "./catalogs";

describe("checkCatalogs", () => {
  it("accepts the bundled catalogs", () => {
    expect(checkCatalogs(bundledCatalogs)).toEqual([]);
  });
  it("reports clean on identical catalogs", () => {
    const issues = checkCatalogs({
      "zh-CN": { "nav.primary": "主要导航", "greet": "你好，{name}" },
      "en-US": { "nav.primary": "Primary navigation", "greet": "Hello, {name}" },
    });
    expect(issues).toEqual([]);
  });

  it("reports a missing key", () => {
    const issues = checkCatalogs({
      "zh-CN": { "a": "A", "b": "B" },
      "en-US": { "a": "A" },
    });
    expect(issues).toEqual([['en-US: 缺少键 "b"']]);
  });

  it("reports an extra key", () => {
    const issues = checkCatalogs({
      "zh-CN": { "a": "A" },
      "en-US": { "a": "A", "extra": "X" },
    });
    expect(issues).toEqual([['en-US: 多余键 "extra"']]);
  });

  it("reports a placeholder mismatch", () => {
    const issues = checkCatalogs({
      "zh-CN": { "greet": "你好，{name}" },
      "en-US": { "greet": "Hello, {nombre}" },
    });
    expect(issues).toEqual([['en-US: 键 "greet" 占位符不一致：源有 {name}，目标没有']]);
  });

  it("reports a placeholder count mismatch", () => {
    const issues = checkCatalogs({
      "zh-CN": { "install": "{version} · {percent}" },
      "en-US": { "install": "{version}" },
    });
    expect(issues).toEqual([
      ['en-US: 键 "install" 占位符不一致：源 {version, percent}，目标 {version}'],
    ]);
  });

  it("reports a plural category missing its .other fallback", () => {
    const issues = checkCatalogs({
      "zh-CN": { "fileCount.one": "{count} 个" },
      "en-US": { "fileCount.one": "{count} file" },
    });
    expect(issues).toEqual([['zh-CN: 复数键 "fileCount.one" 缺少回退类别 "fileCount.other"']]);
  });

  it("reports a missing key alongside a placeholder mismatch independently", () => {
    const issues = checkCatalogs({
      "zh-CN": { "a": "A", "greet": "你好，{name}" },
      "en-US": { "greet": "Hello, {nombre}" },
    });
    expect(issues).toEqual([
      ['en-US: 缺少键 "a"'],
      ['en-US: 键 "greet" 占位符不一致：源有 {name}，目标没有'],
    ]);
  });
});
