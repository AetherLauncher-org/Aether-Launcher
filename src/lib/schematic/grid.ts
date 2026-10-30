import { blockLook } from "./colors";

/** 每个调色板项一个字节的标志位，网格、建网格和指方块都只看这个。 */
export const FLAG_DRAW = 1;
export const FLAG_OCCLUDE = 2;
export const FLAG_TRANSLUCENT = 4;
export const FLAG_PICK = 8;

export type PaletteItem = { name: string };

/**
 * 一块长方体的方块。方块按 `(y * sizeZ + z) * sizeX + x` 排列，和后端一致；调色板的
 * 显示信息（颜色、标志）在构造时算好，之后建网格和指方块都不再碰字符串。
 */
export class VoxelGrid {
  readonly flags: Uint8Array;
  readonly rgb: Uint8Array;

  constructor(
    readonly sx: number,
    readonly sy: number,
    readonly sz: number,
    readonly blocks: Uint16Array,
    palette: readonly PaletteItem[],
  ) {
    if (blocks.length !== sx * sy * sz) {
      throw new Error("方块数量和尺寸对不上");
    }
    this.flags = new Uint8Array(palette.length);
    this.rgb = new Uint8Array(palette.length * 3);
    palette.forEach((item, index) => {
      const look = blockLook(item.name);
      this.flags[index] =
        (look.draw ? FLAG_DRAW : 0) |
        (look.occlude ? FLAG_OCCLUDE : 0) |
        (look.translucent ? FLAG_TRANSLUCENT : 0) |
        (look.pick ? FLAG_PICK : 0);
      this.rgb.set(look.rgb, index * 3);
    });
  }

  /** 方块下标；越界为 -1。 */
  index(x: number, y: number, z: number): number {
    if (x < 0 || y < 0 || z < 0 || x >= this.sx || y >= this.sy || z >= this.sz) {
      return -1;
    }
    return (y * this.sz + z) * this.sx + x;
  }

  /** 调色板序号；越界为 -1。 */
  paletteAt(x: number, y: number, z: number): number {
    const index = this.index(x, y, z);
    return index < 0 ? -1 : this.blocks[index];
  }

  get maxSide(): number {
    return Math.max(this.sx, this.sy, this.sz);
  }
}
