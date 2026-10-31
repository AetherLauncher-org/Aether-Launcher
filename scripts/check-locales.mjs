#!/usr/bin/env node
/**
 * 语言目录一致性检查。以 `locales/zh-CN.json` 为源语言（source of truth），
 * 逐个校验其余语言目录的完整性，供本地和 CI 使用。
 *
 * 检查项：
 *   - 缺失键：源语言有、目标语言没有
 *   - 多余键：目标语言有、源语言没有
 *   - 占位符不一致：同一键两边的 `{name}` 集合不一致
 *   - 复数类别不完整：出现 `<词干>.<类别>` 却没配 `<词干>.other`
 *   - 结构错误：值不是字符串、重复键
 *
 * 用法：`node scripts/check-locales.mjs [localesDir]`（默认 `locales`）。
 * 有问题时退出码为 1，并按语言打印差异；干净时退出码为 0。
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"];
const PLURAL_SUFFIXES = new Set(PLURAL_CATEGORIES);

const SOURCE_LOCALE = "zh-CN";

/** 提取 `{name}` 占位符集合。 */
function placeholders(template) {
  const names = new Set();
  for (const match of template.matchAll(/\{([a-zA-Z_][a-zA-Z0-9_]*)\}/g)) {
    names.add(match[1]);
  }
  return names;
}

/** 把一个 JSON 文本解析成扁平的 `键 -> 字符串` 目录，顺带做结构校验。 */
function parseCatalog(name, text) {
  const issues = [];
  let data;
  try {
    data = JSON.parse(text);
  } catch (error) {
    return { issues: [`${name}: 不是合法 JSON：${error.message}`], entries: new Map() };
  }
  if (data === null || typeof data !== "object" || Array.isArray(data)) {
    return { issues: [`${name}: 顶层必须是对象`], entries: new Map() };
  }

  const entries = new Map();
  const seen = new Set();
  for (const [key, value] of Object.entries(data)) {
    if (typeof value !== "string") {
      issues.push(`${name}: 键 "${key}" 的值不是字符串（复数类别请用平铺键，不要用嵌套对象）`);
      continue;
    }
    if (seen.has(key)) {
      // JSON.parse 对重复键是后者覆盖前者；这里用逐行扫描兜底。
      issues.push(`${name}: 键 "${key}" 重复出现`);
    }
    seen.add(key);
    entries.set(key, value);
  }
  return { issues, entries };
}

/** 逐个文件的行级重复键检查（依赖「一个键一行」的格式约定）。 */
function duplicateKeys(name, text) {
  const issues = [];
  const counts = new Map();
  for (const line of text.split("\n")) {
    const match = line.match(/^\s*"((?:[^"\\]|\\.)*)"\s*:/);
    if (match) {
      counts.set(match[1], (counts.get(match[1]) ?? 0) + 1);
    }
  }
  for (const [key, count] of counts) {
    if (count > 1) {
      issues.push(`${name}: 键 "${key}" 出现 ${count} 次`);
    }
  }
  return issues;
}

/**
 * 纯校验函数（供测试复用）：给定「语言名 -> 目录对象」，返回按语言分组的
 * 问题列表；无问题时返回空数组。
 */
export function checkCatalogs(catalogs) {
  const languages = Object.keys(catalogs);
  const source = catalogs[SOURCE_LOCALE];
  const issues = [];

  if (!source) {
    return [[`缺少源语言目录 "${SOURCE_LOCALE}"`]];
  }

  const sourceKeys = new Set(Object.keys(source));

  // 源语言自身的复数完整性
  for (const key of sourceKeys) {
    const suffix = key.slice(key.lastIndexOf(".") + 1);
    if (PLURAL_SUFFIXES.has(suffix) && suffix !== "other") {
      const stem = key.slice(0, key.lastIndexOf("."));
      if (!sourceKeys.has(`${stem}.other`)) {
        issues.push([`${SOURCE_LOCALE}: 复数键 "${key}" 缺少回退类别 "${stem}.other"`]);
      }
    }
  }

  for (const locale of languages) {
    if (locale === SOURCE_LOCALE) {
      continue;
    }
    const catalog = catalogs[locale];
    const catalogKeys = new Set(Object.keys(catalog));

    for (const key of sourceKeys) {
      if (!catalogKeys.has(key)) {
        issues.push([`${locale}: 缺少键 "${key}"`]);
      }
    }
    for (const key of catalogKeys) {
      if (!sourceKeys.has(key)) {
        issues.push([`${locale}: 多余键 "${key}"`]);
      }
    }
    for (const key of sourceKeys) {
      if (!catalogKeys.has(key)) {
        continue;
      }
      const sourcePlaceholders = placeholders(source[key]);
      const targetPlaceholders = placeholders(catalog[key]);
      if (sourcePlaceholders.size !== targetPlaceholders.size) {
        issues.push([`${locale}: 键 "${key}" 占位符不一致：源 {${[...sourcePlaceholders].join(", ")}}，目标 {${[...targetPlaceholders].join(", ")}}`]);
        continue;
      }
      for (const name of sourcePlaceholders) {
        if (!targetPlaceholders.has(name)) {
          issues.push([`${locale}: 键 "${key}" 占位符不一致：源有 {${name}}，目标没有`]);
        }
      }
    }
  }

  return issues;
}

/** 从磁盘加载目录，返回 `{ catalogs, issues }`。 */
function loadFromDir(dir) {
  const catalogs = {};
  const issues = [];
  let files;
  try {
    files = readdirSync(dir).filter((f) => f.endsWith(".json"));
  } catch (error) {
    return { catalogs, issues: [[`无法读取目录 "${dir}"：${error.message}`]] };
  }
  for (const file of files) {
    const locale = file.slice(0, -".json".length);
    const text = readFileSync(join(dir, file), "utf8");
    const parsed = parseCatalog(locale, text);
    for (const issue of parsed.issues) {
      issues.push([issue]);
    }
    for (const issue of duplicateKeys(locale, text)) {
      issues.push([issue]);
    }
    if (parsed.entries.size > 0 || parsed.issues.length === 0) {
      catalogs[locale] = Object.fromEntries(parsed.entries);
    }
  }
  return { catalogs, issues };
}

function main() {
  const dir = process.argv[2] ? resolve(process.argv[2]) : resolve("locales");
  const { catalogs, issues } = loadFromDir(dir);

  const all = [...issues, ...checkCatalogs(catalogs)];

  if (all.length === 0) {
    console.log(`locales 目录一致：${Object.keys(catalogs).join(", ")}`);
    return;
  }

  for (const group of all) {
    for (const line of group) {
      console.error(line);
    }
  }
  console.error(`共 ${all.length} 处问题`);
  process.exitCode = 1;
}

// 作为脚本直接运行时执行 main；被 import（测试）时不执行。
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
