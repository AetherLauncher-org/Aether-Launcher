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

/**
 * 假装在 Tauri 桌面环境里运行（`isTauri()` 变为 true）。
 *
 * 事件监听要整条链路可用才不报错：`listen` 先用 `transformCallback` 拿一个数字
 * id，再 invoke 注册；退订时走 `__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener`。
 * 少任何一环，挂载了 `InstallProvider` 的用例都会抛 unhandled rejection。
 */
export function pretendDesktop() {
  let callbacks = 0;
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {
      transformCallback: () => {
        callbacks += 1;
        return callbacks;
      },
      unregisterCallback: () => undefined,
      // 这里故意 reject：测试里真实的后端不在，调用方本来就应该走自己的
      // 失败分支。返回 undefined 反而会让「取到了空数据」的分支崩在 .map 上。
      invoke: async () => {
        throw new Error("no backend in tests");
      },
    },
    configurable: true,
  });
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
    value: { unregisterListener: () => undefined },
    configurable: true,
  });
}

export function pretendBrowser() {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  Reflect.deleteProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__");
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
