import { FLAG_DRAW, FLAG_OCCLUDE, FLAG_TRANSLUCENT } from "./grid";
import type { VoxelGrid } from "./grid";

/** 一块网格的边长（方块数）。 */
export const CHUNK = 16;

export type MeshData = {
  /** xyz，每个顶点三个浮点数 */
  positions: Float32Array;
  /** rgb，每个顶点三个字节，已经乘上了朝向的明暗 */
  colors: Uint8Array;
  indices: Uint32Array;
};

export type ChunkMesh = {
  cx: number;
  cy: number;
  cz: number;
  opaque: MeshData | null;
  translucent: MeshData | null;
  /** 合并之后的四边形数（两层加起来） */
  quads: number;
};

/** 朝向的明暗：顶面最亮、底面最暗，不用光照也有立体感。 */
const SHADE: Record<string, number> = {
  "1+": 1,
  "1-": 0.55,
  "0+": 0.82,
  "0-": 0.82,
  "2+": 0.68,
  "2-": 0.68,
};

export function chunkCounts(grid: VoxelGrid): [number, number, number] {
  return [Math.ceil(grid.sx / CHUNK), Math.ceil(grid.sy / CHUNK), Math.ceil(grid.sz / CHUNK)];
}

class Builder {
  positions: number[] = [];
  colors: number[] = [];
  indices: number[] = [];
  quads = 0;

  quad(corners: number[][], rgb: ArrayLike<number>, shade: number): void {
    const base = this.positions.length / 3;
    for (const corner of corners) {
      this.positions.push(corner[0], corner[1], corner[2]);
      this.colors.push(
        Math.round(rgb[0] * shade),
        Math.round(rgb[1] * shade),
        Math.round(rgb[2] * shade),
      );
    }
    this.indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
    this.quads += 1;
  }

  finish(): MeshData | null {
    if (this.quads === 0) {
      return null;
    }
    return {
      positions: new Float32Array(this.positions),
      colors: new Uint8Array(this.colors),
      indices: new Uint32Array(this.indices),
    };
  }
}

/**
 * 一块 16×16×16 网格的生成任务，分成 6 步（三个轴 × 正负两个方向），每步之间可以暂停。
 *
 * 一个面是否可见：它自己要画，并且朝向的那一格不是不透明方块；两块相同的半透明方块
 * 之间的面也不画。纯色方块没有纹理，所以合并只看调色板序号：一整面墙只需要两个三角形，
 * 同色相邻的面用贪心法合并成大四边形。
 *
 * 分步是为了帧率：调度器每帧只给几毫秒，一块再复杂也只需要在这一步里做完六分之一，
 * 不会让某一帧因为一块网格而超时。
 */
export class ChunkJob {
  static readonly STEPS = 6;

  private readonly opaque = new Builder();
  private readonly translucent = new Builder();
  private readonly origin: number[];
  private readonly extent: number[];
  private readonly mask = new Int32Array(CHUNK * CHUNK);
  private pass = 0;

  constructor(
    private readonly grid: VoxelGrid,
    readonly cx: number,
    readonly cy: number,
    readonly cz: number,
  ) {
    this.origin = [cx * CHUNK, cy * CHUNK, cz * CHUNK];
    const limit = [grid.sx, grid.sy, grid.sz];
    this.extent = [0, 1, 2].map((axis) => Math.min(CHUNK, limit[axis] - this.origin[axis]));
  }

  get done(): boolean {
    return this.pass >= ChunkJob.STEPS;
  }

  /** 做一步（一个方向的所有切片）。已经做完则什么都不做。 */
  step(): void {
    if (this.done) {
      return;
    }
    const axis = this.pass >> 1;
    const sign = this.pass % 2 === 0 ? 1 : -1;
    this.pass += 1;
    this.facePass(axis, sign);
  }

  result(): ChunkMesh {
    return {
      cx: this.cx,
      cy: this.cy,
      cz: this.cz,
      opaque: this.opaque.finish(),
      translucent: this.translucent.finish(),
      quads: this.opaque.quads + this.translucent.quads,
    };
  }

  private facePass(axis: number, sign: 1 | -1): void {
    const { grid, origin, extent, mask } = this;
    const u = (axis + 1) % 3;
    const v = (axis + 2) % 3;
    const shade = SHADE[`${axis}${sign > 0 ? "+" : "-"}`];
    for (let slice = 0; slice < extent[axis]; slice += 1) {
      const position = [0, 0, 0];
      // 这一层的遮罩：可见面所属的调色板序号 + 1，没有则为 0。
      for (let j = 0; j < extent[v]; j += 1) {
        for (let i = 0; i < extent[u]; i += 1) {
          position[axis] = origin[axis] + slice;
          position[u] = origin[u] + i;
          position[v] = origin[v] + j;
          const here = grid.paletteAt(position[0], position[1], position[2]);
          let key = 0;
          if (here >= 0 && (grid.flags[here] & FLAG_DRAW) !== 0) {
            position[axis] += sign;
            const there = grid.paletteAt(position[0], position[1], position[2]);
            const hidden =
              there >= 0 &&
              ((grid.flags[there] & FLAG_OCCLUDE) !== 0 ||
                (there === here && (grid.flags[here] & FLAG_TRANSLUCENT) !== 0));
            key = hidden ? 0 : here + 1;
          }
          mask[j * CHUNK + i] = key;
        }
      }
      // 贪心合并：先沿 u 找最宽，再沿 v 找最高。
      for (let j = 0; j < extent[v]; j += 1) {
        for (let i = 0; i < extent[u]; ) {
          const key = mask[j * CHUNK + i];
          if (key === 0) {
            i += 1;
            continue;
          }
          let width = 1;
          while (i + width < extent[u] && mask[j * CHUNK + i + width] === key) {
            width += 1;
          }
          let height = 1;
          grow: while (j + height < extent[v]) {
            for (let k = 0; k < width; k += 1) {
              if (mask[(j + height) * CHUNK + i + k] !== key) {
                break grow;
              }
            }
            height += 1;
          }
          for (let dj = 0; dj < height; dj += 1) {
            for (let di = 0; di < width; di += 1) {
              mask[(j + dj) * CHUNK + i + di] = 0;
            }
          }

          const palette = key - 1;
          const plane = origin[axis] + slice + (sign > 0 ? 1 : 0);
          const at = (du: number, dv: number): number[] => {
            const corner = [0, 0, 0];
            corner[axis] = plane;
            corner[u] = origin[u] + i + du;
            corner[v] = origin[v] + j + dv;
            return corner;
          };
          // 从外面看逆时针：正方向按 (u,v) 顺序，负方向反过来。
          const corners =
            sign > 0
              ? [at(0, 0), at(width, 0), at(width, height), at(0, height)]
              : [at(0, 0), at(0, height), at(width, height), at(width, 0)];
          const target = (grid.flags[palette] & FLAG_TRANSLUCENT) !== 0 ? this.translucent : this.opaque;
          target.quad(corners, grid.rgb.subarray(palette * 3, palette * 3 + 3), shade);
          i += width;
        }
      }
    }
  }
}

/** 一口气做完一块（测试和不在乎时间片的调用方用）。 */
export function meshChunk(grid: VoxelGrid, cx: number, cy: number, cz: number): ChunkMesh {
  const job = new ChunkJob(grid, cx, cy, cz);
  while (!job.done) {
    job.step();
  }
  return job.result();
}
