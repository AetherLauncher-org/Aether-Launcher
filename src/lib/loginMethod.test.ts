import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  defaultLoginMethod,
  isLoginMethod,
  loadLoginMethod,
  loginMethodStorageKey,
  otherLoginMethod,
  saveLoginMethod,
} from "./loginMethod";

describe("loginMethod", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("defaults to the device code flow, which needs no callback address", () => {
    expect(defaultLoginMethod).toBe("device-code");
    expect(loadLoginMethod()).toBe("device-code");
  });

  it("remembers the choice", () => {
    saveLoginMethod("browser");
    expect(window.localStorage.getItem(loginMethodStorageKey)).toBe("browser");
    expect(loadLoginMethod()).toBe("browser");
    saveLoginMethod("device-code");
    expect(loadLoginMethod()).toBe("device-code");
  });

  it("ignores anything it did not write", () => {
    for (const junk of ["", "oauth", "Browser", "null", "{}"]) {
      window.localStorage.setItem(loginMethodStorageKey, junk);
      expect(loadLoginMethod()).toBe("device-code");
    }
    expect(isLoginMethod(undefined)).toBe(false);
    expect(isLoginMethod(1)).toBe(false);
  });

  it("works when storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(loadLoginMethod()).toBe("device-code");
    expect(() => saveLoginMethod("browser")).not.toThrow();
  });

  it("knows the other method", () => {
    expect(otherLoginMethod("device-code")).toBe("browser");
    expect(otherLoginMethod("browser")).toBe("device-code");
  });
});
