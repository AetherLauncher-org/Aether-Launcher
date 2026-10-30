import { FLAG_PICK } from "./grid";
import type { VoxelGrid } from "./grid";

export type Hit = {
  x: number;
  y: number;
  z: number;
  /** 方块下标 `(y * sizeZ + z) * sizeX + x` */
  index: number;
  /** 调色板序号 */
  palette: number;
  /** 光线击中的那个面的外法线 */
  normal: [number, number, number];
  distance: number;
};

/**
 * 从 `origin` 沿 `direction`（不必是单位向量）找第一个可被指到的方块。
 *
 * 体素步进（Amanatides–Woo）：每一步只走到下一个格子边界，开销和光线穿过的格子数成
 * 正比，和方块总数无关，所以每帧做一次也不会让帧超时。光线从方块外面射来时先裁到包围盒。
 */
export function pick(
  grid: VoxelGrid,
  origin: readonly [number, number, number],
  direction: readonly [number, number, number],
  maxDistance: number,
): Hit | null {
  const length = Math.hypot(direction[0], direction[1], direction[2]);
  if (length === 0 || !Number.isFinite(length)) {
    return null;
  }
  const dir = [direction[0] / length, direction[1] / length, direction[2] / length];
  const size = [grid.sx, grid.sy, grid.sz];

  // 先把光线裁到包围盒里（slab 法）。
  let tEnter = 0;
  let tExit = maxDistance;
  let enterAxis = -1;
  for (let axis = 0; axis < 3; axis += 1) {
    if (Math.abs(dir[axis]) < 1e-12) {
      if (origin[axis] < 0 || origin[axis] >= size[axis]) {
        return null;
      }
      continue;
    }
    let t0 = (0 - origin[axis]) / dir[axis];
    let t1 = (size[axis] - origin[axis]) / dir[axis];
    if (t0 > t1) {
      [t0, t1] = [t1, t0];
    }
    if (t0 > tEnter) {
      tEnter = t0;
      enterAxis = axis;
    }
    tExit = Math.min(tExit, t1);
    if (tEnter > tExit) {
      return null;
    }
  }

  // 略微推进一点，免得起点正好落在格子边界上选错格子。
  const start = tEnter + 1e-6;
  const cell = [0, 1, 2].map((axis) =>
    Math.min(size[axis] - 1, Math.max(0, Math.floor(origin[axis] + dir[axis] * start))),
  );
  const step = dir.map((value) => (value > 0 ? 1 : -1));
  const tDelta = dir.map((value) => (Math.abs(value) < 1e-12 ? Infinity : Math.abs(1 / value)));
  const tMax = [0, 1, 2].map((axis) => {
    if (!Number.isFinite(tDelta[axis])) {
      return Infinity;
    }
    const boundary = step[axis] > 0 ? cell[axis] + 1 : cell[axis];
    return (boundary - origin[axis]) / dir[axis];
  });

  // 光线从外面射进包围盒时，第一个格子的面就是进入的那个面；相机在盒子里则没有面。
  let normal: [number, number, number] = [0, 0, 0];
  if (enterAxis >= 0) {
    normal[enterAxis] = -step[enterAxis];
  }
  let travelled = start;
  for (;;) {
    const palette = grid.paletteAt(cell[0], cell[1], cell[2]);
    if (palette >= 0 && (grid.flags[palette] & FLAG_PICK) !== 0) {
      return {
        x: cell[0],
        y: cell[1],
        z: cell[2],
        index: grid.index(cell[0], cell[1], cell[2]),
        palette,
        normal,
        distance: travelled,
      };
    }
    // 走到最近的格子边界。
    const axis = tMax[0] < tMax[1] ? (tMax[0] < tMax[2] ? 0 : 2) : tMax[1] < tMax[2] ? 1 : 2;
    travelled = tMax[axis];
    if (!Number.isFinite(travelled) || travelled > tExit) {
      return null;
    }
    cell[axis] += step[axis];
    tMax[axis] += tDelta[axis];
    if (cell[axis] < 0 || cell[axis] >= size[axis]) {
      return null;
    }
    normal = [0, 0, 0];
    normal[axis] = -step[axis];
  }
}
