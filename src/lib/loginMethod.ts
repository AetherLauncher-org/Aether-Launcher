/**
 * 微软登录用哪种方式。
 *
 * - `device-code`：设备码。启动器给一个短码，用户在任意浏览器（甚至另一台设备）里输入。
 *   默认值，因为它不依赖本机的浏览器能否跳回启动器，也不需要额外的回调地址。
 * - `browser`：浏览器授权（授权码 + PKCE）。启动器直接打开登录页，登录后浏览器跳回本机
 *   回环地址，不用抄码；需要应用注册里登记了 `http://localhost` 回调。
 */
export type LoginMethod = "device-code" | "browser";

export const loginMethodStorageKey = "aether.loginMethod";

export const defaultLoginMethod: LoginMethod = "device-code";

export function isLoginMethod(value: unknown): value is LoginMethod {
  return value === "device-code" || value === "browser";
}

export function otherLoginMethod(method: LoginMethod): LoginMethod {
  return method === "device-code" ? "browser" : "device-code";
}

export const loginMethodLabel: Record<LoginMethod, string> = {
  "device-code": "设备码",
  browser: "浏览器授权",
};

export function loadLoginMethod(): LoginMethod {
  try {
    const stored = window.localStorage.getItem(loginMethodStorageKey);
    return isLoginMethod(stored) ? stored : defaultLoginMethod;
  } catch {
    return defaultLoginMethod;
  }
}

export function saveLoginMethod(method: LoginMethod): void {
  try {
    window.localStorage.setItem(loginMethodStorageKey, method);
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
