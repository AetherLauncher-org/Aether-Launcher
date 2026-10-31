/**
 * 语言偏好持久化，沿用 `src/lib/motion.ts` 的 `aether.*` 模式：
 * `localStorage` 读写在 `try/catch` 里静默回退，未知值用类型守卫挡掉。
 *
 * 原生 UI 落地后，设置会从 `localStorage` 挪到应用数据文件；`load`/`save`
 * 是唯一读写点，替换时只改这里。
 */
import type { Locale } from "./catalogs";
import { defaultLocale, locales } from "./catalogs";

export const localeStorageKey = "aether.locale";

export function isLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" &&
    (locales as readonly string[]).includes(value)
  );
}

export function loadLocale(): Locale {
  try {
    const stored = window.localStorage.getItem(localeStorageKey);
    return isLocale(stored) ? stored : defaultLocale;
  } catch {
    return defaultLocale;
  }
}

export function saveLocale(locale: Locale): void {
  try {
    window.localStorage.setItem(localeStorageKey, locale);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
