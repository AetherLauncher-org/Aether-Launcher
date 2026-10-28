import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";
import type { MockedFunction } from "vitest";
import type { invoke } from "@tauri-apps/api/core";
import type { PageId } from "../navigation";
import { AccountsProvider } from "../state/AccountsProvider";
import { GameProvider } from "../state/GameProvider";
import { InstallProvider } from "../state/InstallProvider";
import { InstancesProvider } from "../state/InstancesProvider";
import { NavigationContext } from "../state/navigation";

type RenderOptions = {
  page?: PageId;
};

/** 在和应用一致的全套 Provider 里渲染一个页面或组件；`navigate` 是个可断言的假函数。 */
export function renderWithState(ui: ReactElement, options: RenderOptions = {}) {
  const navigate = vi.fn();
  const result = render(
    <FluentProvider theme={webLightTheme}>
      <NavigationContext.Provider value={{ current: options.page ?? "home", navigate }}>
        <AccountsProvider>
          <InstancesProvider>
            <InstallProvider>
              <GameProvider>{ui}</GameProvider>
            </InstallProvider>
          </InstancesProvider>
        </AccountsProvider>
      </NavigationContext.Provider>
    </FluentProvider>,
  );
  return { navigate, ...result };
}

/** 假装在 Tauri 桌面环境里运行（`isTauri()` 变为 true）。 */
export function pretendDesktop() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}

export function pretendBrowser() {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
}

export type Handlers = Record<string, (args: unknown) => unknown>;

/**
 * 把 `invoke` 路由到按命令名登记的假后端。没登记的命令会 reject，
 * 这样测试里漏掉的调用会直接暴露，而不是悄悄返回 undefined。
 */
export function routeInvoke(mock: MockedFunction<typeof invoke>, handlers: Handlers) {
  mock.mockImplementation(async (name: string, args?: unknown) => {
    const handler = handlers[name];
    if (handler === undefined) {
      throw new Error(`unexpected command: ${name}`);
    }
    return handler(args);
  });
}
