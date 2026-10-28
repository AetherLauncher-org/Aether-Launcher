import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const current = {
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  setTheme: vi.fn(),
  isMaximized: vi.fn(),
  onResized: vi.fn(),
};
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => current }));

import {
  closeWindow,
  minimizeWindow,
  setWindowTheme,
  toggleMaximizeWindow,
  watchMaximized,
} from "./window";

function pretendDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}

beforeEach(() => {
  for (const mock of Object.values(current)) {
    mock.mockReset();
  }
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("window controls in the browser preview", () => {
  it("do nothing and do not touch the window API", async () => {
    await minimizeWindow();
    await toggleMaximizeWindow();
    await closeWindow();
    await setWindowTheme("dark");
    const stop = await watchMaximized(() => undefined);
    stop();

    for (const mock of Object.values(current)) {
      expect(mock).not.toHaveBeenCalled();
    }
  });
});

describe("window controls in the desktop app", () => {
  it("forward minimize, maximize, close and theme", async () => {
    pretendDesktop();

    await minimizeWindow();
    await toggleMaximizeWindow();
    await closeWindow();
    await setWindowTheme("light");

    expect(current.minimize).toHaveBeenCalledTimes(1);
    expect(current.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(current.close).toHaveBeenCalledTimes(1);
    expect(current.setTheme).toHaveBeenCalledWith("light");
  });

  it("reports the maximized state now and after every resize", async () => {
    pretendDesktop();
    const unlisten = vi.fn();
    let resized: () => void = () => undefined;
    current.isMaximized.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    current.onResized.mockImplementation(async (callback: () => void) => {
      resized = callback;
      return unlisten;
    });
    const seen: boolean[] = [];

    const stop = await watchMaximized((value) => seen.push(value));
    resized();
    await Promise.resolve();
    await Promise.resolve();

    expect(seen).toEqual([false, true]);
    expect(stop).toBe(unlisten);
  });
});
