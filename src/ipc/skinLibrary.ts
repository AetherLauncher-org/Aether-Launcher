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

declare module "./call" {
  interface CommandArgs {
    list_skins: void;
    skin_library_image: { id: string };
    add_skin_from_file: void;
    add_skin_from_player: { name: string };
    rename_skin: { id: string; name: string };
    set_skin_model: { id: string; slim: boolean };
    remove_skin: { id: string };
  }

  interface CommandResults {
    list_skins: SkinEntry[];
    skin_library_image: string;
    add_skin_from_file: AddSkinResult | null;
    add_skin_from_player: AddSkinResult;
    rename_skin: SkinEntry;
    set_skin_model: SkinEntry;
    remove_skin: null;
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

export async function renameSkin(id: string, name: string): Promise<SkinEntry> {
  requireDesktop();
  return callCommand("rename_skin", { id, name });
}

export async function setSkinModel(id: string, slim: boolean): Promise<SkinEntry> {
  requireDesktop();
  return callCommand("set_skin_model", { id, slim });
}

export async function removeSkin(id: string): Promise<void> {
  requireDesktop();
  await callCommand("remove_skin", { id });
}
