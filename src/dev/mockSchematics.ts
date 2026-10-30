/**
 * 浏览器预览用的假投影：几个小建筑，按真实后端的返回形状（调色板 + 游程编码的方块 +
 * 方块实体）生成，让 3D 预览、悬停提示、转换对话框都能在没有 Tauri 的开发服务器里试。
 */

import type { BlockEntityView, ConversionTargets, PaletteEntry, SchematicFormat, SchematicView } from "../ipc/schematics";

type Shape = {
  name: string;
  description: string;
  size: [number, number, number];
  /** 返回方块 ID（`null` 是空气） */
  block: (x: number, y: number, z: number) => string | null;
  entities?: BlockEntityView[];
};

const HEART = ["..XX.XX..", ".XXXXXXX.", "XXXXXXXXX", "XXXXXXXXX", ".XXXXXXX.", "..XXXXX..", "...XXX...", "....X...."];

const SHAPES: Record<string, Shape> = {
  house: {
    name: "Small House",
    description: "A 7x6x7 cobblestone cottage.",
    size: [7, 6, 7],
    block: (x, y, z) => {
      if (y === 0) {
        return "minecraft:oak_planks";
      }
      const edge = x === 0 || x === 6 || z === 0 || z === 6;
      if (y >= 1 && y <= 3 && edge) {
        if (z === 0 && x === 3 && y <= 2) {
          return null;
        }
        if (y === 2 && ((z === 0 || z === 6) && (x === 2 || x === 4))) {
          return "minecraft:glass";
        }
        return "minecraft:cobblestone";
      }
      if (y === 4 || (y === 5 && x >= 1 && x <= 5 && z >= 1 && z <= 5)) {
        return "minecraft:spruce_planks";
      }
      return x === 5 && y === 1 && z === 5 ? "minecraft:chest[facing=west]" : null;
    },
    entities: [
      {
        x: 5,
        y: 1,
        z: 5,
        id: "minecraft:chest",
        snbt: '{\n  id: "minecraft:chest",\n  CustomName: "Loot",\n  Items: [\n    {\n      Slot: 0b,\n      id: "minecraft:diamond_sword",\n      count: 1\n    },\n    {\n      Slot: 4b,\n      id: "minecraft:golden_apple",\n      count: 12\n    }\n  ]\n}',
        items: [
          { slot: 0, id: "minecraft:diamond_sword", count: 1, data: '{\n  "minecraft:enchantments": {\n    levels: {\n      "minecraft:sharpness": 3\n    }\n  }\n}' },
          { slot: 4, id: "minecraft:golden_apple", count: 12, data: null },
        ],
      },
    ],
  },
  tower: {
    name: "Watch Tower",
    description: "A hollow stone brick tower.",
    size: [5, 12, 5],
    block: (x, y, z) => {
      const edge = x === 0 || x === 4 || z === 0 || z === 4;
      if (y === 0) {
        return "minecraft:stone_bricks";
      }
      if (y <= 9) {
        return edge ? "minecraft:stone_bricks" : null;
      }
      if (y === 10) {
        return "minecraft:oak_planks";
      }
      return y === 11 && edge && (x + z) % 2 === 0 ? "minecraft:stone_bricks" : null;
    },
  },
  heart: {
    name: "Pixel Heart",
    description: "A flat red heart.",
    size: [9, 1, 8],
    block: (x, _y, z) => (HEART[z][x] === "X" ? "minecraft:red_concrete" : null),
  },
  platform: {
    name: "Checker Platform 16",
    description: "A 16x16 checkerboard.",
    size: [16, 1, 16],
    block: (x, _y, z) => ((x + z) % 2 === 0 ? "minecraft:white_concrete" : "minecraft:gray_concrete"),
  },
};

function shapeFor(fileName: string): Shape {
  const lower = fileName.toLowerCase();
  const key = Object.keys(SHAPES).find((candidate) => lower.includes(candidate));
  return SHAPES[key ?? "house"];
}

function toBase64(bytes: number[]): string {
  return btoa(String.fromCharCode(...bytes));
}

function parseState(text: string): PaletteEntry {
  const [name, rest] = text.split("[");
  const properties = (rest ?? "")
    .replace("]", "")
    .split(",")
    .filter((pair) => pair.length > 0)
    .map((pair) => {
      const [key, value] = pair.split("=");
      return { key, value };
    });
  return { name, properties, air: false, count: 0 };
}

function formatOf(fileName: string): SchematicFormat {
  if (fileName.endsWith(".schem")) {
    return "schem";
  }
  return fileName.endsWith(".nbt") ? "nbt" : "litematic";
}

export function mockSchematicView(fileName: string): SchematicView {
  const shape = shapeFor(fileName);
  const [sx, sy, sz] = shape.size;
  const palette: PaletteEntry[] = [{ name: "minecraft:air", properties: [], air: true, count: 0 }];
  const indexOf = new Map<string, number>();
  const runs: number[] = [];
  let last = -1;
  let count = 0;
  let solid = 0;
  const flush = () => {
    if (count > 0) {
      runs.push(last & 255, last >> 8, 0, 0, count & 255, (count >> 8) & 255, (count >> 16) & 255, 0);
    }
  };
  for (let y = 0; y < sy; y += 1) {
    for (let z = 0; z < sz; z += 1) {
      for (let x = 0; x < sx; x += 1) {
        const text = shape.block(x, y, z);
        let index = 0;
        if (text !== null) {
          const known = indexOf.get(text);
          if (known === undefined) {
            palette.push(parseState(text));
            index = palette.length - 1;
            indexOf.set(text, index);
          } else {
            index = known;
          }
          solid += 1;
        }
        palette[index].count += 1;
        if (index === last) {
          count += 1;
        } else {
          flush();
          last = index;
          count = 1;
        }
      }
    }
  }
  flush();
  return {
    fileName,
    format: formatOf(fileName),
    name: shape.name,
    author: "Aether Launcher samples",
    description: shape.description,
    dataVersion: 3955,
    size: shape.size,
    solidBlocks: solid,
    palette,
    runs: toBase64(runs),
    runCount: runs.length / 8,
    blockEntities: shape.entities ?? [],
    blockEntityTotal: shape.entities?.length ?? 0,
  };
}

export const mockConversionTargets: ConversionTargets = {
  formats: [
    { label: "litematic", name: "Litematica (.litematic)" },
    { label: "schem", name: "Sponge / WorldEdit (.schem)" },
    { label: "nbt", name: "原版结构文件 (.nbt)" },
  ],
  versions: [
    { dataVersion: 3465, name: "1.20.1" },
    { dataVersion: 3839, name: "1.20.6" },
    { dataVersion: 3955, name: "1.21.1" },
    { dataVersion: 4189, name: "1.21.4" },
  ],
};
