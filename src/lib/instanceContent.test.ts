import { describe, expect, it } from "vitest";
import type { ContentItem, ContentKind } from "../ipc/manage";
import {
  contentCopy,
  describeLog,
  describeRemoval,
  displayName,
  formatBytes,
  formatModified,
} from "./instanceContent";

const file = (name: string): ContentItem => ({ name, sizeBytes: 1, enabled: true, isFolder: false });
const folder = (name: string): ContentItem => ({ name, sizeBytes: 1, enabled: true, isFolder: true });

describe("instanceContent", () => {
  it("has copy for every kind of content", () => {
    const kinds: ContentKind[] = ["mods", "resourcepacks", "shaderpacks", "saves", "schematics"];
    for (const kind of kinds) {
      expect(contentCopy[kind].tab.length).toBeGreaterThan(0);
      expect(contentCopy[kind].emptyBody.length).toBeGreaterThan(0);
    }
  });

  it("formats sizes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(250 * 1024 * 1024)).toBe("250 MB");
    expect(formatBytes(3.5 * 1024 ** 3)).toBe("3.5 GB");
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });

  it("formats a modification time, or nothing", () => {
    expect(formatModified(null)).toBe("");
    expect(formatModified(Number.NaN)).toBe("");
    expect(formatModified(0)).toMatch(/^19(69|70)-\d\d-\d\d \d\d:\d\d$/);
  });

  it("shows a mod's name without its jar and disabled suffixes", () => {
    expect(displayName("mods", file("sodium-0.5.jar"))).toBe("sodium-0.5");
    expect(displayName("mods", file("sodium-0.5.jar.disabled"))).toBe("sodium-0.5");
    expect(displayName("mods", file("Sodium.JAR"))).toBe("Sodium");
  });

  it("drops .zip from packs, and leaves worlds and schematics alone", () => {
    expect(displayName("resourcepacks", file("Faithful.zip"))).toBe("Faithful");
    expect(displayName("shaderpacks", file("BSL.zip"))).toBe("BSL");
    expect(displayName("resourcepacks", folder("Unpacked.zip"))).toBe("Unpacked.zip");
    expect(displayName("saves", folder("My World"))).toBe("My World");
    expect(displayName("schematics", file("House.litematic"))).toBe("House.litematic");
  });

  it("describes logs", () => {
    expect(describeLog({ kind: "latest", name: "latest.log", sizeBytes: 1, modified: null })).toContain("latest.log");
    expect(describeLog({ kind: "crash", name: "crash-x.txt", sizeBytes: 1, modified: null })).toBe("crash-x.txt");
    expect(describeLog({ kind: "crash", name: "crash-x.txt", sizeBytes: 1, modified: 1_700_000_000 })).toMatch(
      /^崩溃报告 \d{4}-\d\d-\d\d \d\d:\d\d$/,
    );
  });

  it("warns that deleting a world takes everything in it", () => {
    expect(describeRemoval("saves", folder("World"))).toContain("所有区块");
    expect(describeRemoval("mods", file("a.jar"))).toBe("「a」会从实例里删除，无法恢复。");
  });
});
