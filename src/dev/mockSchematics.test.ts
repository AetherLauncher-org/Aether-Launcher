import { describe, expect, it } from "vitest";
import { decodeRuns } from "../lib/schematic/decode";
import { mockConversionTargets, mockSchematicView } from "./mockSchematics";

describe("mockSchematicView", () => {
  it("has the shape the backend returns: blocks that add up to the size, and counts that add up", () => {
    for (const name of ["Small House.litematic", "Watch Tower.litematic", "Pixel Heart.litematic", "Checker Platform 16.litematic"]) {
      const view = mockSchematicView(name);
      const volume = view.size[0] * view.size[1] * view.size[2];
      const blocks = decodeRuns(view.runs, view.runCount, volume);

      expect(blocks).toHaveLength(volume);
      expect(view.palette[0].air).toBe(true);
      expect(view.palette.reduce((sum, entry) => sum + entry.count, 0)).toBe(volume);
      expect(view.solidBlocks).toBe(volume - view.palette[0].count);
      expect(blocks.every((index) => index < view.palette.length)).toBe(true);
    }
  });

  it("puts a chest with items in the house, in a position that holds a chest", () => {
    const view = mockSchematicView("Small House.litematic");
    const [sx, , sz] = view.size;
    const chest = view.blockEntities[0];
    const blocks = decodeRuns(view.runs, view.runCount, sx * view.size[1] * sz);

    expect(view.palette[blocks[(chest.y * sz + chest.z) * sx + chest.x]].name).toBe("minecraft:chest");
    expect(chest.items.length).toBeGreaterThan(0);
    expect(view.blockEntityTotal).toBe(1);
  });

  it("follows the file name for the format, and falls back to the house", () => {
    expect(mockSchematicView("a.schem").format).toBe("schem");
    expect(mockSchematicView("a.nbt").format).toBe("nbt");
    expect(mockSchematicView("unknown thing.litematic").name).toBe("Small House");
  });

  it("offers every format and some versions", () => {
    expect(mockConversionTargets.formats.map((f) => f.label)).toEqual(["litematic", "schem", "nbt"]);
    expect(mockConversionTargets.versions.length).toBeGreaterThan(0);
  });
});
