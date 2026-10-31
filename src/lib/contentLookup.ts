/**
 * 要不要到 Modrinth 上识别模组、资源包、光影包（显示名称、简介、图标）。
 *
 * 识别时离开本机的只有文件的 SHA-1 哈希，不含文件名和路径。默认开启；关掉之后卡片只显示
 * 磁盘上的文件名，不发任何请求。
 */
export const contentLookupStorageKey = "aether.contentLookup";

export function loadContentLookup(): boolean {
  try {
    return window.localStorage.getItem(contentLookupStorageKey) !== "off";
  } catch {
    return true;
  }
}

export function saveContentLookup(enabled: boolean): void {
  try {
    window.localStorage.setItem(contentLookupStorageKey, enabled ? "on" : "off");
  } catch {
    // 存储不可用时只在本次运行内生效
  }
}
