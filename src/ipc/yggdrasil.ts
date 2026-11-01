import { callCommand } from "./call";
import { isTauri } from "./index";

export type YggdrasilProfile = { id: string; name: string };
export type YggdrasilLogin = { id: string; profiles: YggdrasilProfile[] };
export type YggdrasilAccount = { id: string; server: string; profile: YggdrasilProfile };

export const yggdrasilPresets = [
  { name: "LittleSkin", server: "https://littleskin.cn/api/yggdrasil" },
  { name: "Ely.by", server: "https://account.ely.by/api/authlib-injector" },
] as const;

/** Input convenience only; the backend enforces DNS and network policy. */
export function isYggdrasilServer(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password && !url.search && !url.hash;
  } catch {
    return false;
  }
}

declare module "./call" {
  interface CommandArgs {
    begin_yggdrasil_login: { server: string; username: string; password: string };
    finish_yggdrasil_login: { id: string; profileId: string };
    cancel_yggdrasil_login: { id: string };
    list_yggdrasil_accounts: void;
    refresh_yggdrasil_account: { id: string };
    remove_yggdrasil_account: { id: string };
  }
  interface CommandResults {
    begin_yggdrasil_login: YggdrasilLogin;
    finish_yggdrasil_login: YggdrasilAccount;
    cancel_yggdrasil_login: null;
    list_yggdrasil_accounts: YggdrasilAccount[];
    refresh_yggdrasil_account: null;
    remove_yggdrasil_account: null;
  }
}

// Password is inbound only; do not log, cache or persist this command's arguments.
export const beginYggdrasilLogin = (server: string, username: string, password: string) =>
  callCommand("begin_yggdrasil_login", { server, username, password });
export const finishYggdrasilLogin = (id: string, profileId: string) =>
  callCommand("finish_yggdrasil_login", { id, profileId });
export const cancelYggdrasilLogin = (id: string) => callCommand("cancel_yggdrasil_login", { id });
/**
 * 本机已登录的第三方账户。读不到（浏览器预览、后端出错）时返回 null，而不是空数组：
 * 调用方据此区分「确实没有账户」和「不知道」，不会因为一次读取失败就清空账户簿。
 */
export async function listYggdrasilAccounts(): Promise<YggdrasilAccount[] | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    return await callCommand("list_yggdrasil_accounts");
  } catch {
    return null;
  }
}
export const refreshYggdrasilAccount = (id: string) => callCommand("refresh_yggdrasil_account", { id });
export const removeYggdrasilAccount = (id: string) => callCommand("remove_yggdrasil_account", { id });

const errors: Record<string, string> = {
  "authlib-java-required": "第三方账户需要 Java 8 或更新版本，请选择兼容的游戏版本。",
  "authlib-invalid-path": "认证代理的缓存路径无法用于启动，请检查应用数据目录。",
  "authlib-network-error": "无法获取认证代理（authlib-injector），请检查网络后重试。",
  "authlib-download-error": "没有可用的认证代理下载源，请稍后重试。",
  "authlib-cache-invalid": "本地认证代理文件校验失败，请删除后重新下载。",
  "authlib-cache-conflict": "已存在同版本但内容不同的认证代理文件，请清理应用数据目录后重试。",
  "authlib-storage-error": "无法写入认证代理缓存，请检查应用数据目录权限。",
  "yggdrasil-storage-error": "无法访问系统凭据库，请检查系统凭据管理后重试。",
  "yggdrasil-network-error": "连接不上认证服务器，请检查网络后重试。",
  "yggdrasil-busy": "已有账户操作正在进行，请稍后重试。",
  "yggdrasil-invalid-server": "请输入有效的 HTTPS API 根地址，不要包含账号、查询参数或片段。",
  "yggdrasil-invalid-input": "请检查用户名和密码后重试。",
  "yggdrasil-no-profile": "这个账户还没有角色，请先到认证服务的网站创建角色。",
  "yggdrasil-invalid-profile": "所选角色不可用，请重新登录。",
  "yggdrasil-login-expired": "这次登录已过期，请关闭对话框后重新开始。",
  "yggdrasil-sign-in-required": "登录已失效，请重新添加此账户。",
  "yggdrasil-invalid-account": "找不到此账户，请重新加载账户列表。",
  "yggdrasil-response-too-large": "认证服务器的响应过大，无法处理。",
  "yggdrasil-service-error": "认证服务器拒绝了请求，请检查登录信息或稍后重试。",
  "yggdrasil-invalid-response": "认证服务器返回了无法识别的数据。",
  "yggdrasil-invalid-metadata": "此地址没有提供有效的认证服务信息。",
};

export function describeYggdrasilError(error: unknown): string {
  const code = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return Object.prototype.hasOwnProperty.call(errors, code) ? errors[code] : "第三方账户操作失败，请稍后重试。";
}
