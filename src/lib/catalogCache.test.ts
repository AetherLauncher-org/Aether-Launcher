import { beforeEach, describe, expect, it, vi } from "vitest";

const listMinecraftVersions = vi.fn();
vi.mock("../ipc", () => ({ listMinecraftVersions: () => listMinecraftVersions() }));

import { loadCatalog, resetCatalogCache } from "./catalogCache";

const catalog = { latest: { release: "1.21.1", snapshot: "24w14a" }, versions: [] };

beforeEach(() => {
  listMinecraftVersions.mockReset();
  resetCatalogCache();
});

describe("loadCatalog", () => {
  it("reads the catalog once and serves later calls from memory", async () => {
    listMinecraftVersions.mockResolvedValue(catalog);

    await expect(loadCatalog()).resolves.toBe(catalog);
    await expect(loadCatalog()).resolves.toBe(catalog);

    expect(listMinecraftVersions).toHaveBeenCalledTimes(1);
  });

  it("re-reads when forced and keeps the fresh copy", async () => {
    listMinecraftVersions.mockResolvedValueOnce(catalog).mockResolvedValueOnce({ ...catalog, latest: { release: "1.21.2", snapshot: "x" } });

    await loadCatalog();
    const fresh = await loadCatalog(true);
    expect(fresh.latest.release).toBe("1.21.2");
    expect((await loadCatalog()).latest.release).toBe("1.21.2");
    expect(listMinecraftVersions).toHaveBeenCalledTimes(2);
  });

  it("does not cache a failure", async () => {
    listMinecraftVersions.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(catalog);

    await expect(loadCatalog()).rejects.toThrow("offline");
    await expect(loadCatalog()).resolves.toBe(catalog);
  });
});
