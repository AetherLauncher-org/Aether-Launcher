import { FLAG_DRAW, FLAG_OCCLUDE, FLAG_TRANSLUCENT } from "./grid";
import type { VoxelGrid } from "./grid";

/**
 * 卡片上的缩略图：等距（2:1）投影，从东南上方看。后画的盖住先画的，所以按「由后到前」
 * 的顺序画：先低层再高层，同一层按 x+z 从小到大。只画看得见的三个面（顶面、+x 面、+z 面），
 * 被不透明方块挡住的面直接跳过。
 *
 * 画一张图可能有几十万个四边形，所以做成可以分段执行的任务：每帧只给几毫秒，
 * 做不完下一帧再接着做，不会让页面卡一下。
 */

/** 体积超过这么多方块的投影不画缩略图（点开 3D 预览看）。 */
export const MAX_THUMBNAIL_VOLUME = 1_500_000;

/** 画布上下文里用到的那几个方法，方便测试时换成假的。 */
export type DrawContext = {
  fillStyle: string;
  globalAlpha: number;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  closePath(): void;
  fill(): void;
};

const SHADE = { top: 1, east: 0.82, south: 0.68 };

function colour(grid: VoxelGrid, palette: number, shade: number): string {
  const r = Math.round(grid.rgb[palette * 3] * shade);
  const g = Math.round(grid.rgb[palette * 3 + 1] * shade);
  const b = Math.round(grid.rgb[palette * 3 + 2] * shade);
  return `rgb(${r},${g},${b})`;
}

/** 方块是否会挡住它背后的面。 */
function hides(grid: VoxelGrid, x: number, y: number, z: number): boolean {
  const palette = grid.paletteAt(x, y, z);
  return palette >= 0 && (grid.flags[palette] & FLAG_OCCLUDE) !== 0;
}

export class IsoJob {
  private readonly scale: number;
  private readonly offsetX: number;
  private readonly offsetY: number;
  private y = 0;
  private d = 0;
  private x = 0;
  private started = false;
  private finished = false;
  /** 已经画了的四边形数（测试和调试用）。 */
  quads = 0;

  constructor(
    private readonly grid: VoxelGrid,
    private readonly ctx: DrawContext,
    width: number,
    height: number,
    padding = 6,
  ) {
    // 投影后的包围盒。X = (x - z) * 1，Y = (x + z) * 0.5 - y，单位是半个方块宽。
    const { sx, sy, sz } = grid;
    const minX = -sz;
    const maxX = sx;
    const minY = -sy;
    const maxY = (sx + sz) / 2;
    this.scale = Math.min((width - 2 * padding) / (maxX - minX), (height - 2 * padding) / (maxY - minY));
    this.offsetX = padding + (width - 2 * padding - (maxX - minX) * this.scale) / 2 - minX * this.scale;
    this.offsetY = padding + (height - 2 * padding - (maxY - minY) * this.scale) / 2 - minY * this.scale;
  }

  get done(): boolean {
    return this.finished;
  }

  private px(x: number, z: number): number {
    return this.offsetX + (x - z) * this.scale;
  }

  private py(x: number, y: number, z: number): number {
    return this.offsetY + ((x + z) * 0.5 - y) * this.scale;
  }

  private face(corners: [number, number, number][], style: string, translucent: boolean): void {
    const { ctx } = this;
    ctx.fillStyle = style;
    ctx.globalAlpha = translucent ? 0.55 : 1;
    ctx.beginPath();
    corners.forEach(([x, y, z], index) => {
      const sx = this.px(x, z);
      const sy = this.py(x, y, z);
      if (index === 0) {
        ctx.moveTo(sx, sy);
      } else {
        ctx.lineTo(sx, sy);
      }
    });
    ctx.closePath();
    ctx.fill();
    this.quads += 1;
  }

  private block(x: number, y: number, z: number): void {
    const { grid } = this;
    const palette = grid.paletteAt(x, y, z);
    if (palette < 0 || (grid.flags[palette] & FLAG_DRAW) === 0) {
      return;
    }
    const translucent = (grid.flags[palette] & FLAG_TRANSLUCENT) !== 0;
    if (!hides(grid, x, y + 1, z)) {
      this.face([[x, y + 1, z], [x + 1, y + 1, z], [x + 1, y + 1, z + 1], [x, y + 1, z + 1]], colour(grid, palette, SHADE.top), translucent);
    }
    if (!hides(grid, x + 1, y, z)) {
      this.face([[x + 1, y, z], [x + 1, y, z + 1], [x + 1, y + 1, z + 1], [x + 1, y + 1, z]], colour(grid, palette, SHADE.east), translucent);
    }
    if (!hides(grid, x, y, z + 1)) {
      this.face([[x, y, z + 1], [x + 1, y, z + 1], [x + 1, y + 1, z + 1], [x, y + 1, z + 1]], colour(grid, palette, SHADE.south), translucent);
    }
  }

  /**
   * 画到 `deadline`（`performance.now()` 的时间点）为止，返回是否画完。每画 64 个方块才看一次
   * 时间，免得查时间本身成了开销。
   */
  run(deadline: number, now: () => number = () => performance.now()): boolean {
    const { sx, sy, sz } = this.grid;
    if (!this.started) {
      this.started = true;
      this.x = Math.max(0, this.d - (sz - 1));
    }
    let sinceCheck = 0;
    while (this.y < sy) {
      const diagonals = sx + sz - 1;
      while (this.d < diagonals) {
        const last = Math.min(sx - 1, this.d);
        while (this.x <= last) {
          this.block(this.x, this.y, this.d - this.x);
          this.x += 1;
          sinceCheck += 1;
          if (sinceCheck >= 64) {
            sinceCheck = 0;
            if (now() >= deadline) {
              return false;
            }
          }
        }
        this.d += 1;
        this.x = Math.max(0, this.d - (sz - 1));
      }
      this.y += 1;
      this.d = 0;
      this.x = 0;
    }
    this.finished = true;
    return true;
  }
}
