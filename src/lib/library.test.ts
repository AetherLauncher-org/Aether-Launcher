import { describe, expect, it } from "vitest";
import type { ResourceEntry } from "./library";
import {
  aggregateSources,
  compareVersions,
  describeResourceKind,
  filterResources,
  formatBytes,
  sortResources,
  summariseResources,
} from "./library";

function resource(overrides: Partial<ResourceEntry> = {}): ResourceEntry {
  return {
    id: "p1",
    name: "JEI",
    gameVersion: "1.21.1",
    kind: "mod",
    source: "modrinth",
    host: "cdn.modrinth.com",
    sha1: null,
    size: 1024,
    ...overrides,
  };
}

describe("describeResourceKind", () => {
  it("labels every kind in Chinese", () => {
    expect(describeResourceKind("mod")).toBe("模组");
    expect(describeResourceKind("resourcepack")).toBe("资源包");
    expect(describeResourceKind("shader")).toBe("光影");
    expect(describeResourceKind("datapack")).toBe("数据包");
  });
});

describe("aggregateSources", () => {
  it("counts entries per source and sorts by count", () => {
    const facets = aggregateSources([
      resource({ id: "a", source: "modrinth" }),
      resource({ id: "b", source: "modrinth" }),
      resource({ id: "c", source: "curseforge" }),
    ]);

    expect(facets).toEqual([
      { source: "modrinth", count: 2 },
      { source: "curseforge", count: 1 },
    ]);
  });

  it("breaks ties by source name", () => {
    const facets = aggregateSources([
      resource({ id: "a", source: "curseforge" }),
      resource({ id: "b", source: "modrinth" }),
    ]);

    expect(facets.map((facet) => facet.source)).toEqual([
      "curseforge",
      "modrinth",
    ]);
  });

  it("returns nothing for an empty list", () => {
    expect(aggregateSources([])).toEqual([]);
  });
});

describe("filterResources", () => {
  const entries = [
    resource({ id: "a", name: "JEI", source: "modrinth", gameVersion: "1.21.1" }),
    resource({
      id: "b",
      name: "Sodium",
      source: "curseforge",
      gameVersion: "1.20.1",
    }),
    resource({
      id: "c",
      name: "补丁资源包",
      source: "modrinth",
      gameVersion: "1.21.1",
    }),
  ];

  it("returns everything when unfiltered", () => {
    expect(filterResources(entries, { query: "", source: null })).toHaveLength(
      3,
    );
  });

  it("filters by source", () => {
    const filtered = filterResources(entries, { query: "", source: "modrinth" });
    expect(filtered.map((entry) => entry.id)).toEqual(["a", "c"]);
  });

  it("filters by query across name, version and source", () => {
    expect(
      filterResources(entries, { query: "sodium", source: null }).map(
        (entry) => entry.id,
      ),
    ).toEqual(["b"]);
    expect(
      filterResources(entries, { query: "1.20", source: null }).map(
        (entry) => entry.id,
      ),
    ).toEqual(["b"]);
    expect(
      filterResources(entries, { query: "curseforge", source: null }).map(
        (entry) => entry.id,
      ),
    ).toEqual(["b"]);
  });

  it("combines source and query", () => {
    expect(
      filterResources(entries, { query: "sodium", source: "modrinth" }),
    ).toEqual([]);
  });

  it("matches non-latin names", () => {
    expect(
      filterResources(entries, { query: "补丁", source: null }).map(
        (entry) => entry.id,
      ),
    ).toEqual(["c"]);
  });
});

describe("sortResources", () => {
  const entries = [
    resource({ id: "a", name: "B", size: 300, gameVersion: "1.20.1" }),
    resource({ id: "b", name: "A", size: 100, gameVersion: "1.21.1" }),
    resource({ id: "c", name: "C", size: 200, gameVersion: "1.19.4" }),
  ];

  it("sorts by name", () => {
    expect(sortResources(entries, "name").map((entry) => entry.id)).toEqual([
      "b",
      "a",
      "c",
    ]);
  });

  it("sorts by size", () => {
    expect(sortResources(entries, "size").map((entry) => entry.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
  });

  it("sorts by game version numerically", () => {
    expect(
      sortResources(entries, "gameVersion").map((entry) => entry.id),
    ).toEqual(["c", "a", "b"]);
  });

  it("does not mutate the input", () => {
    const input = [...entries];
    sortResources(input, "size");
    expect(input.map((entry) => entry.id)).toEqual(["a", "b", "c"]);
  });
});

describe("compareVersions", () => {
  it("compares each numeric segment", () => {
    expect(compareVersions("1.21.1", "1.20.10")).toBeGreaterThan(0);
    expect(compareVersions("1.20.1", "1.21")).toBeLessThan(0);
    expect(compareVersions("1.21", "1.21.0")).toBe(0);
  });

  it("handles prefixes and suffixes", () => {
    expect(compareVersions("1.20.5-pre1", "1.20.5")).toBeLessThan(0);
    expect(compareVersions("1.20.5", "1.20.5-pre1")).toBeGreaterThan(0);
  });
});

describe("formatBytes", () => {
  it("keeps small files in bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("uses KB, MB and GB", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(1024 * 1024 * 1024 * 2)).toBe("2.0 GB");
  });

  it("drops the decimal for large numbers", () => {
    expect(formatBytes(150 * 1024)).toBe("150 KB");
  });

  it("rejects negative or non-finite input", () => {
    expect(formatBytes(-1)).toBe("未知大小");
    expect(formatBytes(Number.NaN)).toBe("未知大小");
  });
});

describe("summariseResources", () => {
  it("says so when empty", () => {
    expect(summariseResources([])).toBe("还没有可用的资源");
  });

  it("counts entries and sources", () => {
    expect(
      summariseResources([
        resource({ id: "a", source: "modrinth" }),
        resource({ id: "b", source: "curseforge" }),
      ]),
    ).toBe("共 2 项 · 2 个来源");
  });
});
