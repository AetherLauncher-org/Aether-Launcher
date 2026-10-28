import { describe, expect, it } from "vitest";
import { isMotionPreference, loadMotionPreference, motionStorageKey, saveMotionPreference } from "./motion";

describe("motion preference", () => {
  it("defaults to full animation", () => {
    expect(loadMotionPreference()).toBe("full");
  });

  it("remembers a saved choice", () => {
    saveMotionPreference("reduced");
    expect(window.localStorage.getItem(motionStorageKey)).toBe("reduced");
    expect(loadMotionPreference()).toBe("reduced");
  });

  it("ignores a corrupted stored value", () => {
    window.localStorage.setItem(motionStorageKey, "turbo");
    expect(loadMotionPreference()).toBe("full");
  });

  it("recognises only the two valid values", () => {
    expect(isMotionPreference("full")).toBe(true);
    expect(isMotionPreference("reduced")).toBe(true);
    expect(isMotionPreference("system")).toBe(false);
    expect(isMotionPreference(null)).toBe(false);
  });
});
