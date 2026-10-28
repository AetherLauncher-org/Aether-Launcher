import { describe, expect, it } from "vitest";
import type { MinecraftCatalogVersion } from "../ipc";
import {
  availability,
  filterVersions,
  formatReleaseDate,
  formatReleaseDateTime,
  kindOf,
  metadataHost,
  modernArgumentsCutoff,
  suggestInstanceName,
} from "./versions";

function version(
  id: string,
  type = "release",
  releaseTime = "2024-08-08T13:34:05+00:00",
): MinecraftCatalogVersion {
  return {
    id,
    type,
    releaseTime,
    sha1: "0123456789abcdef0123456789abcdef01234567",
    url: `https://piston-meta.mojang.com/v1/packages/x/${id}.json`,
  };
}

describe("kindOf", () => {
  it("groups the four official types into three kinds", () => {
    expect(kindOf({ type: "release" })).toBe("release");
    expect(kindOf({ type: "snapshot" })).toBe("snapshot");
    expect(kindOf({ type: "old_beta" })).toBe("old");
    expect(kindOf({ type: "old_alpha" })).toBe("old");
  });

  it("treats an unknown type as a snapshot so it never disappears", () => {
    expect(kindOf({ type: "experiment" })).toBe("snapshot");
  });
});

describe("availability", () => {
  it("accepts versions that use the structured arguments format", () => {
    expect(availability(version("1.21.1"))).toEqual({ ok: true });
    expect(availability(version("17w43a", "snapshot", `${modernArgumentsCutoff}T00:00:00+00:00`))).toEqual({
      ok: true,
    });
  });

  it("rejects versions released before the cutoff with a reason", () => {
    const result = availability(version("1.12.2", "release", "2017-09-18T08:39:46+00:00"));
    expect(result.ok).toBe(false);
    expect(result.ok ? "" : result.reason).toContain("1.13");
  });

  it("rejects the ancient alpha and beta versions", () => {
    expect(availability(version("b1.7.3", "old_beta", "2011-07-08T00:00:00+00:00")).ok).toBe(false);
    // 即使日期晚于分界线，远古版也不支持
    expect(availability(version("weird", "old_alpha", "2024-01-01T00:00:00+00:00")).ok).toBe(false);
  });
});

describe("filterVersions", () => {
  const all = [
    version("1.21.1"),
    version("24w14a", "snapshot"),
    version("1.21"),
    version("b1.7.3", "old_beta"),
    version("1.20.4"),
  ];

  it("keeps only the chosen kind, in catalog order", () => {
    expect(filterVersions(all, { kind: "release", query: "" }).map((item) => item.id)).toEqual([
      "1.21.1",
      "1.21",
      "1.20.4",
    ]);
    expect(filterVersions(all, { kind: "snapshot", query: "" }).map((item) => item.id)).toEqual([
      "24w14a",
    ]);
    expect(filterVersions(all, { kind: "old", query: "" }).map((item) => item.id)).toEqual(["b1.7.3"]);
  });

  it("searches the id case-insensitively and ignores surrounding spaces", () => {
    expect(filterVersions(all, { kind: "release", query: "  1.21 " }).map((item) => item.id)).toEqual([
      "1.21.1",
      "1.21",
    ]);
    expect(filterVersions(all, { kind: "snapshot", query: "24W" }).map((item) => item.id)).toEqual([
      "24w14a",
    ]);
  });

  it("returns an empty list when nothing matches", () => {
    expect(filterVersions(all, { kind: "release", query: "9.9" })).toEqual([]);
  });
});

describe("date and host formatting", () => {
  it("keeps the date part of an ISO timestamp", () => {
    expect(formatReleaseDate("2024-08-08T13:34:05+00:00")).toBe("2024-08-08");
    expect(formatReleaseDate("garbage")).toBe("garbage");
  });

  it("formats a full local date and time, and passes bad input through", () => {
    expect(formatReleaseDateTime("2024-08-08T13:34:05+00:00")).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
    expect(formatReleaseDateTime("not a date")).toBe("not a date");
  });

  it("reports the host that serves the metadata", () => {
    expect(metadataHost("https://piston-meta.mojang.com/v1/packages/x/1.json")).toBe("piston-meta.mojang.com");
    expect(metadataHost("not a url")).toBe("未知");
  });
});

describe("suggestInstanceName", () => {
  it("uses the version id when it is free", () => {
    expect(suggestInstanceName("1.21.1", ["Other"])).toBe("1.21.1");
  });

  it("appends the first free number, ignoring case", () => {
    expect(suggestInstanceName("1.21.1", ["1.21.1"])).toBe("1.21.1 (2)");
    expect(suggestInstanceName("1.21.1", ["1.21.1", "1.21.1 (2)", "1.21.1 (3)"])).toBe("1.21.1 (4)");
    expect(suggestInstanceName("24w14a", ["24W14A"])).toBe("24w14a (2)");
  });
});
