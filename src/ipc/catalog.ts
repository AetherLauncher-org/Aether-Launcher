import { callCommand } from "./call";
import { isTauri } from "./index";

/** Mojang Java Edition version_manifest_v2.json, validated by the Rust backend. */
export type MinecraftCatalogVersion = {
  id: string;
  type: never;
  kind: string;
  url: string;
  sha1: string;
  releaseTime: string;
};

export type MinecraftVersionCatalog = {
  latest: { release: string; snapshot: string };
  versions: MinecraftCatalogVersion[];
};

declare module "./call" {
  interface CommandArgs {
    list_minecraft_versions: void;
  }

  interface CommandResults {
    list_minecraft_versions: MinecraftVersionCatalog;
  }
}

/** Official catalog discovery is deliberately unavailable in the browser preview. */
export function listMinecraftVersions(): Promise<MinecraftVersionCatalog> {
  if (!isTauri()) {
    return Promise.reject(new Error("需要桌面版才能读取官方版本目录"));
  }
  return callCommand("list_minecraft_versions");
}
