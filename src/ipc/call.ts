import { invoke } from "@tauri-apps/api/core";

/**
 * 带参数的 IPC 调用。
 *
 * `src/ipc/index.ts` 对每个命令单独写封装，命令一多就要重复一遍
 * 「取名字 + 传参数 + 标注返回类型」。这里把三件事收进一个泛型函数：
 * 命令名、参数、返回值都由 `CommandArgs` / `CommandResults` 两张登记表决定，
 * 名字写错、参数漏字段、返回值类型写错都会在编译期报出来。
 *
 * 两张表都是可扩展的映射类型（`interface` + 声明合并），各领域的命令
 * 登记在自己的文件里，再由主 lead 在 `index.ts` 合并：
 *
 * ```ts
 * // src/ipc/instances.ts
 * declare module "./call" {
 *   interface CommandArgs {
 *     list_instances: void;
 *     create_instance: { name: string; version: string };
 *   }
 * }
 *
 * // src/ipc/index.ts
 * export type { CommandArgs, CommandResults } from "./call";
 * ```
 *
 * 用 `interface` 而不是 `type` 是因为只有接口声明可以跨模块合并。
 * 不传参数的命令在 `CommandArgs` 里写 `void`，调用时省略第二个参数。
 */

/** 每个命令接收的参数。没有参数的命令登记为 `void`。 */
export interface CommandArgs {
  application_info: void;
}

/** 每个命令的返回类型。新增命令先在后端注册，再补到这张表。 */
export interface CommandResults {
  application_info: unknown;
}

/** 登记为 `void` 的命令不接受第二个参数。 */
type CallRest<Name extends keyof CommandArgs> = CommandArgs[Name] extends void
  ? []
  : [args: CommandArgs[Name]];

/**
 * 调用一个后端命令。
 *
 * 参数原样传给 `invoke`：Tauri 2 把对象按 camelCase 的键名映射到 Rust
 * 命令的 snake_case 参数，所以调用方写的是 TypeScript 习惯的 `{ name }`。
 */
export function callCommand<
  Name extends keyof CommandArgs & keyof CommandResults,
>(name: Name, ...rest: CallRest<Name>): Promise<CommandResults[Name]> {
  const args = rest[0];
  const pending =
    args === undefined ? invoke<CommandResults[Name]>(name) : invoke<CommandResults[Name]>(name, args);
  if (import.meta.env.DEV && import.meta.env.MODE !== "test") {
    traceCall(name, args, pending);
  }
  return pending;
}

/** 超过这么久还没回来的调用，在控制台警告一次（毫秒）。 */
const SLOW_CALL_MS = 5000;

/**
 * 只在开发模式：把每次 IPC 调用记到控制台——发出、回来（耗时、结果是什么类型）、失败、
 * 以及超过 5 秒还没回来的。**只记命令名和参数的键名，不记参数的值**：有的命令带着密码
 * 之类的东西，日志里不该出现。界面「一直转圈」时，用它能一眼看出调用是没发出去、没回来，
 * 还是回来了但界面没用上。
 */
function traceCall(name: string, args: unknown, pending: Promise<unknown>): void {
  const keys = typeof args === "object" && args !== null ? Object.keys(args).join(",") : "";
  const started = performance.now();
  const elapsed = () => Math.round(performance.now() - started);
  console.debug(`[ipc] → ${name}(${keys})`);
  const slow = setTimeout(() => {
    console.warn(`[ipc] ${name} has not answered after ${SLOW_CALL_MS / 1000} s`);
  }, SLOW_CALL_MS);
  // 这条分支自己处理了失败，不会多出一个未处理的拒绝；调用方拿到的仍是原来的 promise
  pending.then(
    (value) => {
      clearTimeout(slow);
      const kind = value === null ? "null" : Array.isArray(value) ? `array(${value.length})` : typeof value;
      console.debug(`[ipc] ← ${name} ok in ${elapsed()} ms, ${kind}`);
    },
    (error: unknown) => {
      clearTimeout(slow);
      console.debug(`[ipc] ✗ ${name} failed in ${elapsed()} ms:`, error);
    },
  );
}

/**
 * 把后端返回的错误整理成能显示在界面上的文字。
 *
 * Tauri 命令的 `Result::Err` 会以字符串 reject；网络层、序列化之类的
 * 失败则是 `Error`。页面不该把 `[object Object]` 或堆栈直接展示给用户。
 */
export function describeCommandError(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  if (error === null || error === undefined) {
    return "未知错误";
  }
  if (typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string" && message.length > 0) {
      return message;
    }
  }
  return String(error);
}
