import { isTauri } from "./index";

/**
 * 账户相关的 IPC。
 *
 * 目前后端只有账户模型（`aether-auth` 的 `AccountProfile`），设备码登录流程
 * 还在开发中，所以这里**没有**发起任何命令：`startMicrosoftLogin()` 只负责
 * 报告当前环境能不能登录，避免界面上出现一个点了没反应的按钮。
 *
 * 等 `src-tauri` 里注册了登录命令（例如 `start_microsoft_login`），
 * 把下面的实现换成 `callCommand("start_microsoft_login")` 即可，
 * 返回类型的含义保持不变：**只暴露阶段，永远不返回令牌**。
 */

/** 登录流程对外暴露的阶段。故意没有 token 字段。 */
export type MicrosoftLoginStatus =
  | { kind: "unavailable"; reason: string }
  | { kind: "not-implemented"; reason: string };

/**
 * 尝试开始微软登录。
 *
 * 浏览器预览：没有 Tauri 运行时，明确告诉用户要用桌面版。
 * 桌面版：命令还没注册，告诉用户流程仍在开发。
 *
 * 这里刻意不 reject —— 两个分支都是「预期内的不可用」，页面按普通状态渲染提示，
 * 而不是弹错误。
 */
export function startMicrosoftLogin(): Promise<MicrosoftLoginStatus> {
  if (!isTauri()) {
    return Promise.resolve({
      kind: "unavailable",
      reason: "微软登录需要桌面版，浏览器预览里无法完成授权。",
    });
  }

  return Promise.resolve({
    kind: "not-implemented",
    reason: "微软设备码登录还在开发中，完成后会在这里打开授权页面。",
  });
}

/** 把状态转成界面上要显示的一句话。 */
export function describeLoginStatus(status: MicrosoftLoginStatus): string {
  return status.reason;
}
