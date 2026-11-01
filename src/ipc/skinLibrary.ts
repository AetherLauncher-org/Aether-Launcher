import { callCommand } from "./call";
import { isTauri } from "./index";

/**
 * 皮肤库的 IPC：本机保存的皮肤，可以从文件（系统对话框选）或玩家名添加，改名、改模型、删除。
 * 命令定义在 `src-tauri/src/commands/skin_library.rs`，规则在 `aether_app::skin_library`。
 *
 * **前端从不传路径**：皮肤用 id（图片内容的 SHA-256）指定；选文件走后端开的系统对话框，
 * 玩家名在后端换成 Mojang 官方服务的请求。每张图在后端验过（PNG、64×64 或 64×32、64 KiB
 * 以内，只看文件头）才保存或给界面，界面拿到的是 `data:` 图片。
 */

export type SkinEntry = {
  /** 图片内容的 SHA-256，64 位小写十六进制 */
  id: string;
  name: string;
  /** 纤细手臂（Alex）；否则是经典（Steve） */
  slim: boolean;
  source: "file" | "player" | "download";
  /** 自 Unix 纪元起的秒数 */
  added: number;
};

export type AddSkinResult = {
  entry: SkinEntry;
  /** false：库里已经有这张一模一样的图，什么也没变 */
  added: boolean;
};

/** 下载页自动取皮肤的来源：都有官方公开接口。 */
export type SkinSource = "mojang" | "littleskin" | "elyby";

/** 没有官方接口、只能「在浏览器里打开」的皮肤站。 */
export type SkinSite = "namemc" | "skindex" | "planetminecraft";

/** 一次搜索的结果：预览图和后端保存这次结果的不透明令牌。 */
export type SkinPreview = {
  /** 只在本会话里有效；不是路径，也不是下载地址 */
  token: string;
  source: SkinSource;
  name: string;
  slim: boolean;
  /** 已验过的 `data:` PNG */
  image: string;
};

/** 保存到选定文件夹的结果：只有文件名，没有路径。 */
export type SavedSkin = {
  fileName: string;
};

declare module "./call" {
  interface CommandArgs {
    list_skins: void;
    skin_library_image: { id: string };
    add_skin_from_file: void;
    add_skin_from_player: { name: string };
    rename_skin: { id: string; name: string };
    set_skin_model: { id: string; slim: boolean };
    remove_skin: { id: string };
    wear_skin: { id: string; accountId: string };
    search_skin: { source: SkinSource; name: string };
    add_downloaded_skin: { token: string };
    save_downloaded_skin: { token: string };
    open_skin_site: { site: SkinSite };
  }

  interface CommandResults {
    list_skins: SkinEntry[];
    skin_library_image: string;
    add_skin_from_file: AddSkinResult | null;
    add_skin_from_player: AddSkinResult;
    rename_skin: SkinEntry;
    set_skin_model: SkinEntry;
    remove_skin: null;
    wear_skin: null;
    search_skin: SkinPreview;
    add_downloaded_skin: AddSkinResult;
    save_downloaded_skin: SavedSkin | null;
    open_skin_site: null;
  }
}

function requireDesktop(): void {
  if (!isTauri()) {
    throw new Error("皮肤库需要桌面版。");
  }
}

export async function listSkins(): Promise<SkinEntry[]> {
  requireDesktop();
  return callCommand("list_skins");
}

/** 一张皮肤的图（`data:` PNG）。 */
export async function getSkinImage(id: string): Promise<string> {
  requireDesktop();
  return callCommand("skin_library_image", { id });
}

/** 弹出系统对话框选一张 PNG 加进皮肤库；取消了返回 null。 */
export async function addSkinFromFile(): Promise<AddSkinResult | null> {
  requireDesktop();
  return callCommand("add_skin_from_file");
}

/** 把一个玩家现在穿的皮肤加进皮肤库（从 Mojang 官方服务取）。 */
export async function addSkinFromPlayer(name: string): Promise<AddSkinResult> {
  requireDesktop();
  return callCommand("add_skin_from_player", { name });
}

/**
 * 在下载页按玩家名找一个皮肤。来源只限有官方接口的 Mojang / LittleSkin / Ely.by；
 * 地址在后端按来源拼好，返回的令牌只在本会话里有效。
 */
export async function searchSkin(source: SkinSource, name: string): Promise<SkinPreview> {
  requireDesktop();
  return callCommand("search_skin", { source, name });
}

/** 把搜索到的皮肤加进皮肤库，来源记为「下载」。 */
export async function addDownloadedSkin(token: string): Promise<AddSkinResult> {
  requireDesktop();
  return callCommand("add_downloaded_skin", { token });
}

/** 保存到用户选的文件夹；取消返回 null，文件夹路径不进前端。 */
export async function saveDownloadedSkin(token: string): Promise<SavedSkin | null> {
  requireDesktop();
  return callCommand("save_downloaded_skin", { token });
}

/** 用系统浏览器打开 NameMC / Skindex / Planet Minecraft 的固定首页，不抓取它们。 */
export async function openSkinSite(site: SkinSite): Promise<void> {
  requireDesktop();
  await callCommand("open_skin_site", { site });
}

export async function renameSkin(id: string, name: string): Promise<SkinEntry> {
  requireDesktop();
  return callCommand("rename_skin", { id, name });
}

export async function setSkinModel(id: string, slim: boolean): Promise<SkinEntry> {
  requireDesktop();
  return callCommand("set_skin_model", { id, slim });
}

/**
 * 把皮肤库里的一张皮肤换到一个微软账户上。账户的令牌只在后端用，界面只传两个 id；
 * 失败时抛出一句不含令牌的原因（要重新登录、Mojang 不接受、请稍后等）。
 */
export async function wearSkin(id: string, accountId: string): Promise<void> {
  requireDesktop();
  await callCommand("wear_skin", { id, accountId });
}

export async function removeSkin(id: string): Promise<void> {
  requireDesktop();
  await callCommand("remove_skin", { id });
}
