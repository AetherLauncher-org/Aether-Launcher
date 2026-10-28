import { afterEach, describe, expect, it } from "vitest";
import { detectMica, isWindows11OrLater } from "./platform";

type Stub = { getHighEntropyValues: (hints: string[]) => Promise<{ platformVersion?: string }> };

function stubUserAgentData(value: Stub | undefined) {
  Object.defineProperty(navigator, "userAgentData", { value, configurable: true });
}

function pretendDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  Reflect.deleteProperty(navigator, "userAgentData");
});

describe("isWindows11OrLater", () => {
  it("treats platform version 13 and above as Windows 11", () => {
    expect(isWindows11OrLater("13.0.0")).toBe(true);
    expect(isWindows11OrLater("15.0.0")).toBe(true);
    expect(isWindows11OrLater("10.0.0")).toBe(false);
    expect(isWindows11OrLater("0.3.0")).toBe(false);
  });

  it("is false for missing or unreadable versions", () => {
    expect(isWindows11OrLater(undefined)).toBe(false);
    expect(isWindows11OrLater("")).toBe(false);
    expect(isWindows11OrLater("abc")).toBe(false);
  });
});

describe("detectMica", () => {
  it("is always false in a plain browser", async () => {
    stubUserAgentData({ getHighEntropyValues: async () => ({ platformVersion: "15.0.0" }) });
    await expect(detectMica()).resolves.toBe(false);
  });

  it("is true in the desktop app on Windows 11", async () => {
    pretendDesktop();
    stubUserAgentData({ getHighEntropyValues: async () => ({ platformVersion: "15.0.0" }) });
    await expect(detectMica()).resolves.toBe(true);
  });

  it("is false on Windows 10, without client hints, or when the lookup fails", async () => {
    pretendDesktop();
    stubUserAgentData({ getHighEntropyValues: async () => ({ platformVersion: "10.0.0" }) });
    await expect(detectMica()).resolves.toBe(false);

    stubUserAgentData(undefined);
    await expect(detectMica()).resolves.toBe(false);

    stubUserAgentData({
      getHighEntropyValues: async () => {
        throw new Error("blocked");
      },
    });
    await expect(detectMica()).resolves.toBe(false);
  });
});
