import { getAccountSkin } from "../ipc";
import { canDrawHead } from "./skinHead";

/**
 * 取一个玩家的皮肤图，整个运行期里同一个玩家只取一次（导航栏和账户页共用）。
 *
 * 后端给的是 `data:` 的 PNG（只来自 Mojang 的皮肤服务，已经验过是 64×64 或 64×32 的 PNG），
 * 这里解码成图。没有皮肤、取不到、或者这台机器画不了，都得到 null，头像就退回字母。
 * 得到 null 的不记住，下次挂载再问一次（后端有自己的磁盘缓存，再问很便宜）。
 */
const loaded = new Map<string, Promise<HTMLImageElement | null>>();

function decode(url: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = url;
  });
}

export function loadSkin(uuid: string): Promise<HTMLImageElement | null> {
  const known = loaded.get(uuid);
  if (known !== undefined) {
    return known;
  }
  const pending = (async () => {
    if (!canDrawHead()) {
      return null;
    }
    try {
      const url = await getAccountSkin(uuid);
      return url === null ? null : await decode(url);
    } catch {
      return null;
    }
  })();
  loaded.set(uuid, pending);
  void pending.then((image) => {
    if (image === null && loaded.get(uuid) === pending) {
      loaded.delete(uuid);
    }
  });
  return pending;
}

/** 测试里用：忘掉所有已取的皮肤。 */
export function forgetSkins(): void {
  loaded.clear();
}
