import { listMinecraftVersions } from "../ipc";
import type { MinecraftVersionCatalog } from "../ipc";

/**
 * 版本目录在一次运行内只读一次：来回切换页面不会重复请求 Mojang。
 * 用户点「刷新」时传 `force` 才会重新读取。
 */
let cached: MinecraftVersionCatalog | null = null;

export async function loadCatalog(force = false): Promise<MinecraftVersionCatalog> {
  if (cached !== null && !force) {
    return cached;
  }
  const catalog = await listMinecraftVersions();
  cached = catalog;
  return catalog;
}

export function resetCatalogCache(): void {
  cached = null;
}
