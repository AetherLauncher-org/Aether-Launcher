import type { Locale } from "./catalogs";
import { defaultLocale, locales } from "./catalogs";

export const localeStorageKey = "aether.locale";

/** 用户的选择：一种具体语言，或「跟随系统」。 */
export type LocalePreference = Locale | "system";

export function isLocale(value: unknown): value is Locale {
  return (
    typeof value === "string" &&
    (locales as readonly string[]).includes(value)
  );
}

export function isPreference(value: unknown): value is LocalePreference {
  return value === "system" || isLocale(value);
}

/**
 * 系统语言对应的界面语言：按系统偏好顺序取第一个能认的，中文用简体中文，英文用 English。
 * 一个都不认（或读不到）时：有系统语言就用 English（通用的后备），完全读不到就用默认语言。
 */
export function systemLocale(languages?: readonly string[]): Locale {
  const list =
    languages ?? (typeof navigator === "undefined" ? [] : navigator.languages ?? [navigator.language]);
  for (const tag of list) {
    const language = tag.toLowerCase().split("-")[0];
    if (language === "zh") {
      return "zh-CN";
    }
    if (language === "en") {
      return "en-US";
    }
  }
  return list.length > 0 ? "en-US" : defaultLocale;
}

export function resolveLocale(preference: LocalePreference, languages?: readonly string[]): Locale {
  return preference === "system" ? systemLocale(languages) : preference;
}

/** 读保存的选择；没有保存过（或值不认识）就是「跟随系统」。 */
export function loadPreference(): LocalePreference {
  try {
    const stored = window.localStorage.getItem(localeStorageKey);
    return isPreference(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

export function savePreference(preference: LocalePreference): void {
  try {
    window.localStorage.setItem(localeStorageKey, preference);
  } catch {
    // 这次运行里选择仍然生效。
  }
}
