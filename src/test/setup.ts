import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Fluent UI v9 的 Dialog / Slider / Tooltip 依赖 ResizeObserver，jsdom 没有实现。
// 补一个空实现，只满足构造和订阅调用；尺寸变化在测试里不需要真的派发。
class ResizeObserverStub {
  observe() {}

  unobserve() {}

  disconnect() {}
}

if (!("ResizeObserver" in window)) {
  Object.defineProperty(window, "ResizeObserver", {
    value: ResizeObserverStub,
    configurable: true,
    writable: true,
  });
}

// vitest 没开 globals，RTL 不会自动清理，这里手动处理
afterEach(() => {
  cleanup();
  window.localStorage.clear();
});
