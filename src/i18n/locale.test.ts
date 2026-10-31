import { afterEach, describe, expect, it } from "vitest";
import { isLocale, loadLocale, localeStorageKey, saveLocale } from "./locale";

describe("locale persistence", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("defaults to zh-CN when nothing is stored", () => {
    expect(loadLocale()).toBe("zh-CN");
  });

  it("remembers a saved locale", () => {
    saveLocale("en-US");
    expect(window.localStorage.getItem(localeStorageKey)).toBe("en-US");
    expect(loadLocale()).toBe("en-US");
  });

  it("ignores an unknown stored value", () => {
    window.localStorage.setItem(localeStorageKey, "fr-FR");
    expect(loadLocale()).toBe("zh-CN");
  });

  it("type guard accepts only registered locales", () => {
    expect(isLocale("zh-CN")).toBe(true);
    expect(isLocale("en-US")).toBe(true);
    expect(isLocale("fr-FR")).toBe(false);
    expect(isLocale(null)).toBe(false);
    expect(isLocale(42)).toBe(false);
  });
});
