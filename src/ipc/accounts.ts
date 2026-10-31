import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 微软账户登录的 IPC。
 *
 * 命令定义在 `src-tauri/src/commands/microsoft.rs`（只做参数转换），登录流程在 `crates/aether-app/src/microsoft.rs`。登录是设备码流程：
 * 有两种登录方式，用户在设置里选：设备码（`beginMicrosoftLogin()` 返回一个短码，用户在
 * 浏览器里输入）和浏览器授权（`beginMicrosoftBrowserLogin()`，后端自己打开登录页，
 * 授权码 + PKCE，浏览器跳回本机回环地址）。之后后端在后台走完 Xbox 与 Minecraft 的
 * 步骤，结果都通过 `login://status` 事件推过来。
 *
 * **这里没有任何令牌。** 前端只看到账户身份（名字、UUID）、设备码的展示值和稳定的
 * 失败分类；访问令牌和刷新令牌留在后端，刷新令牌在系统凭据库里。
 */

export type MicrosoftAccount = {
  /** 启动器的账户 ID（Minecraft 档案 UUID，带连字符） */
  id: string;
  displayName: string;
  /** Minecraft 档案 UUID，32 位小写十六进制 */
  uuid: string;
  xuid: string | null;
};

/** 用户需要在浏览器里完成登录的信息。 */
export type DeviceCodeInfo = {
  userCode: string;
  verificationUri: string;
  expiresInSecs: number;
};

/** 浏览器授权登录的信息：没有码要显示，登录页由后端打开。 */
export type BrowserLoginInfo = {
  expiresInSecs: number;
};

export type LoginFailureReason =
  | "expired"
  | "denied"
  | "network"
  | "service"
  | "storage"
  | "no-xbox-profile"
  | "child-account"
  | "region"
  | "not-entitled"
  | "no-profile";

export type LoginFailure = {
  reason: LoginFailureReason;
  /** 英文的一句话，没有任何秘密；只在不认识 `reason` 时兜底显示 */
  message: string;
};

/** 登录过程中后端推送的状态。 */
export type LoginEvent =
  | { state: "waiting" }
  | { state: "signed-in"; account: MicrosoftAccount }
  | { state: "failed"; failure: LoginFailure }
  | { state: "cancelled" };

export const loginStatusEvent = "login://status";

/** 后端用这几段固定文本表示需要界面特殊处理的错误。 */
export const microsoftNotConfigured = "microsoft-not-configured";
export const microsoftSignInRequired = "microsoft-sign-in-required";
export const microsoftNotEntitled = "microsoft-not-entitled";

declare module "./call" {
  interface CommandArgs {
    microsoft_login_available: void;
    begin_microsoft_login: void;
    begin_microsoft_browser_login: void;
    cancel_microsoft_login: void;
    open_microsoft_verification_page: void;
    list_microsoft_accounts: void;
    remove_microsoft_account: { id: string };
    account_skin: { uuid: string };
  }

  interface CommandResults {
    microsoft_login_available: boolean;
    begin_microsoft_login: DeviceCodeInfo;
    begin_microsoft_browser_login: BrowserLoginInfo;
    cancel_microsoft_login: boolean;
    open_microsoft_verification_page: null;
    list_microsoft_accounts: MicrosoftAccount[];
    remove_microsoft_account: null;
    account_skin: string | null;
  }
}

const desktopOnly = "微软登录需要桌面版，浏览器预览里无法完成授权。";

/** 这个构建能不能登录（有没有配置 Azure 应用 ID）。浏览器预览里是 false。 */
export async function isMicrosoftLoginAvailable(): Promise<boolean> {
  if (!isTauri()) {
    return false;
  }
  try {
    return await callCommand("microsoft_login_available");
  } catch {
    return false;
  }
}

/** 申请设备码并开始后台轮询。 */
export function beginMicrosoftLogin(): Promise<DeviceCodeInfo> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("begin_microsoft_login");
}

/**
 * 打开微软登录页并等浏览器跳回来（授权码 + PKCE）。登录页的地址、端口和密钥都由后端
 * 生成，前端什么都不传。
 */
export function beginMicrosoftBrowserLogin(): Promise<BrowserLoginInfo> {
  if (!isTauri()) {
    return Promise.reject(new Error(desktopOnly));
  }
  return callCommand("begin_microsoft_browser_login");
}

/** 取消正在进行的登录；返回之前是否有登录在进行。 */
export function cancelMicrosoftLogin(): Promise<boolean> {
  if (!isTauri()) {
    return Promise.resolve(false);
  }
  return callCommand("cancel_microsoft_login");
}

/** 在默认浏览器里打开本次登录的验证页面。页面地址由后端决定，前端不传。 */
export async function openMicrosoftVerificationPage(): Promise<void> {
  if (!isTauri()) {
    throw new Error(desktopOnly);
  }
  await callCommand("open_microsoft_verification_page");
}

/**
 * 本机已登录的微软账户。读不到（浏览器预览、后端出错）时返回 null，而不是空数组：
 * 调用方据此区分「确实没有账户」和「不知道」，不会因为一次读取失败就清空账户簿。
 */
export async function listMicrosoftAccounts(): Promise<MicrosoftAccount[] | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    return await callCommand("list_microsoft_accounts");
  } catch {
    return null;
  }
}

/**
 * 玩家皮肤的 PNG（`data:` URL），用来画账户头像。后端只从 Mojang 的皮肤服务取，验过是
 * 64×64 或 64×32 的 PNG；没有自定义皮肤、取不到、或在浏览器预览里，得到 null。不涉及令牌：
 * 玩家 UUID 是公开的。
 */
export async function getAccountSkin(uuid: string): Promise<string | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    return await callCommand("account_skin", { uuid });
  } catch {
    return null;
  }
}

/** 退出一个微软账户：后端删除凭据库里的刷新令牌和账户信息。 */
export async function removeMicrosoftAccount(id: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  await callCommand("remove_microsoft_account", { id });
}

/** 订阅登录状态。浏览器预览里没有事件源，返回一个空的取消函数。 */
export async function listenLoginStatus(
  handler: (event: LoginEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => undefined;
  }
  return listen<LoginEvent>(loginStatusEvent, (event) => handler(event.payload));
}

const failureText: Record<LoginFailureReason, string> = {
  expired: "这次登录已过期，请重新开始。",
  denied: "你在浏览器里拒绝了这次登录。",
  network: "连接不上微软的登录服务，请检查网络后重试。",
  service: "微软或 Mojang 的登录服务拒绝了这次登录，请稍后重试。",
  storage: "账户已验证，但无法保存到系统凭据库。请检查系统的凭据管理是否可用。",
  "no-xbox-profile":
    "这个微软账户还没有 Xbox 档案。先到 xbox.com 登录一次创建档案，再回来登录。",
  "child-account":
    "这是未成年人账户。需要成年人先把它加入微软家庭组，才能联机游玩。",
  region: "这个账户所在的地区不支持 Xbox Live。",
  "not-entitled":
    "这个微软账户没有购买 Minecraft: Java 版。可以先用离线账户，或换一个账户登录。",
  "no-profile": "这个账户还没有 Minecraft 玩家名。先在官方启动器里创建一个。",
};

/**
 * 失败时额外显示的一行详情：后端的英文说明，里面写着是哪一步、什么状态码（例如
 * 「Minecraft services rejected the token」）。后端保证它不含令牌、响应体和完整网址。
 * 只有「服务拒绝」和「网络」这两类需要，别的分类本身已经说清楚了。
 */
export function describeLoginDetail(failure: LoginFailure): string | null {
  return (failure.reason === "service" || failure.reason === "network") && failure.message.length > 0
    ? failure.message
    : null;
}

/** 把登录失败翻译成可以直接显示的一句话；不认识的分类显示后端的英文。 */
export function describeLoginFailure(failure: LoginFailure): string {
  return failureText[failure.reason] ?? failure.message;
}

/**
 * 把启动或登录命令的错误文本翻译成人话。不是微软相关的错误原样返回，
 * 所以可以对任何命令错误调用。
 */
export function describeMicrosoftError(message: string): string {
  switch (message) {
    case microsoftNotConfigured:
      return "这个版本还没有配置微软登录。";
    case microsoftSignInRequired:
      return "微软账户的登录已失效，请到「账户」页重新登录。";
    case microsoftNotEntitled:
      return failureText["not-entitled"];
    default:
      return message;
  }
}
