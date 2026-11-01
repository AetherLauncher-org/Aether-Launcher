/**
 * 皮肤的 3D 模型：身体的每个部位是一个立方体（里层），帽子、外套、袖子、裤腿（外层）按
 * 「3D Skin Layers」的做法做成体素：外层每个不透明的像素是一个有厚度的小方块，凸在里层外面，
 * 侧面也看得见。这里只有几何和旋转，没有画布，所以可以单元测试；画出来见 `skinRender.ts`。
 *
 * 坐标：单位是皮肤的一个像素，x 向右（观察者的右边，也就是人物的左边），y 向上，z 朝向观察者
 * （人物的正面朝 +z）。脚底在 y = 0，头顶在 y = 32。人物的右臂在 x < 0 一侧。
 */

export type Vec3 = readonly [number, number, number];

export type Face = "front" | "back" | "left" | "right" | "top" | "bottom";
export const faceList: readonly Face[] = ["front", "back", "left", "right", "top", "bottom"];

/** 皮肤图上的一块。 */
export type Region = { sx: number; sy: number; sw: number; sh: number };

export type Box = {
  name: string;
  x0: number;
  x1: number;
  y0: number;
  y1: number;
  z0: number;
  z1: number;
  /** 这个立方体在皮肤图上展开图的左上角 */
  u: number;
  v: number;
  /** 立方体的宽（x）、高（y）、深（z），也就是展开图的尺寸 */
  w: number;
  h: number;
  d: number;
  /** 旧式皮肤的左臂左腿：用右边那一块的图，左右镜像 */
  mirror: boolean;
};

/** 一个部位：里层的立方体，加上可能有的外层（帽子、外套、袖子、裤腿）。 */
export type Group = { base: Box; outer: Box | null; thickness: number };

export function faceNormal(face: Face): Vec3 {
  switch (face) {
    case "front":
      return [0, 0, 1];
    case "back":
      return [0, 0, -1];
    case "left":
      return [1, 0, 0];
    case "right":
      return [-1, 0, 0];
    case "top":
      return [0, 1, 0];
    case "bottom":
      return [0, -1, 0];
  }
}

function swapSide(face: Face): Face {
  return face === "left" ? "right" : face === "right" ? "left" : face;
}

/** 展开图上各面的位置：`[右][前][左][后]` 一排，顶面在前面的上方，底面在顶面右边。 */
function regionOf(face: Face, u: number, v: number, w: number, d: number, h: number): Region {
  switch (face) {
    case "top":
      return { sx: u + d, sy: v, sw: w, sh: d };
    case "bottom":
      return { sx: u + d + w, sy: v, sw: w, sh: d };
    case "front":
      return { sx: u + d, sy: v + d, sw: w, sh: h };
    case "right":
      return { sx: u, sy: v + d, sw: d, sh: h };
    case "left":
      return { sx: u + d + w, sy: v + d, sw: d, sh: h };
    case "back":
      return { sx: u + 2 * d + w, sy: v + d, sw: w, sh: h };
  }
}

/** 某个面用皮肤图上的哪一块。镜像的立方体左右两面互换。 */
export function boxRegion(box: Box, face: Face): Region {
  return regionOf(box.mirror ? swapSide(face) : face, box.u, box.v, box.w, box.d, box.h);
}

/** 一个面上的像素在空间里怎么排：第 (0,0) 个像素的角，以及 i、j 增加一格时走的方向。 */
export type Frame = { origin: Vec3; u: Vec3; v: Vec3 };

function plainFrame(box: Box, face: Face): Frame {
  const { x0, x1, y0, y1, z0, z1 } = box;
  switch (face) {
    case "front":
      return { origin: [x0, y1, z1], u: [1, 0, 0], v: [0, -1, 0] };
    case "back":
      return { origin: [x1, y1, z0], u: [-1, 0, 0], v: [0, -1, 0] };
    case "right":
      return { origin: [x0, y1, z0], u: [0, 0, 1], v: [0, -1, 0] };
    case "left":
      return { origin: [x1, y1, z1], u: [0, 0, -1], v: [0, -1, 0] };
    case "top":
      return { origin: [x0, y1, z0], u: [1, 0, 0], v: [0, 0, 1] };
    case "bottom":
      // 底面的图相对顶面上下翻过：v 从前往后
      return { origin: [x0, y0, z1], u: [1, 0, 0], v: [0, 0, -1] };
  }
}

export function boxFrame(box: Box, face: Face): Frame {
  if (!box.mirror) {
    return plainFrame(box, face);
  }
  // 镜像 = 取对面那一面的排法，再沿 x 翻过来
  const frame = plainFrame(box, swapSide(face));
  return {
    origin: [box.x0 + box.x1 - frame.origin[0], frame.origin[1], frame.origin[2]],
    // `0 - x` and not `-x`: a negated zero is `-0`, which is not what a test or a reader expects
    u: [0 - frame.u[0], frame.u[1], frame.u[2]],
    v: [0 - frame.v[0], frame.v[1], frame.v[2]],
  };
}

/** 外层（体素）凸出里层多少个像素。帽子厚一点，别的薄一点，和游戏里的比例接近。 */
export const hatThickness = 1;
export const limbThickness = 0.5;

/**
 * 要画的部位。`modern` 是 64×64（左臂左腿有自己的图，有全部外层）；否则是旧式 64×32：
 * 左臂左腿用右边那块镜像，外层只有帽子。`slim` 是纤细手臂（3 像素宽）。
 */
export function skinGroups(modern: boolean, slim: boolean): Group[] {
  const arm = slim ? 3 : 4;
  const box = (
    name: string,
    x0: number,
    x1: number,
    y0: number,
    y1: number,
    z0: number,
    z1: number,
    u: number,
    v: number,
    mirror = false,
  ): Box => ({ name, x0, x1, y0, y1, z0, z1, u, v, w: x1 - x0, h: y1 - y0, d: z1 - z0, mirror });
  const group = (base: Box, outer: Box | null, thickness = limbThickness): Group => ({ base, outer, thickness });

  return [
    group(box("rightLeg", -4, 0, 0, 12, -2, 2, 0, 16), modern ? box("rightPants", -4, 0, 0, 12, -2, 2, 0, 32) : null),
    group(
      modern ? box("leftLeg", 0, 4, 0, 12, -2, 2, 16, 48) : box("leftLeg", 0, 4, 0, 12, -2, 2, 0, 16, true),
      modern ? box("leftPants", 0, 4, 0, 12, -2, 2, 0, 48) : null,
    ),
    group(box("body", -4, 4, 12, 24, -2, 2, 16, 16), modern ? box("jacket", -4, 4, 12, 24, -2, 2, 16, 32) : null),
    group(
      box("rightArm", -4 - arm, -4, 12, 24, -2, 2, 40, 16),
      modern ? box("rightSleeve", -4 - arm, -4, 12, 24, -2, 2, 40, 32) : null,
    ),
    group(
      modern ? box("leftArm", 4, 4 + arm, 12, 24, -2, 2, 32, 48) : box("leftArm", 4, 4 + arm, 12, 24, -2, 2, 40, 16, true),
      modern ? box("leftSleeve", 4, 4 + arm, 12, 24, -2, 2, 48, 48) : null,
    ),
    group(box("head", -4, 4, 24, 32, -4, 4, 0, 0), box("hat", -4, 4, 24, 32, -4, 4, 32, 0), hatThickness),
  ];
}

/** `width`×`height` 的皮肤图是不是 64×64 的新式皮肤（旧式是 64×32）。 */
export function isModernSkin(width: number, height: number): boolean {
  return width === 64 && height === 64;
}

// --------------------------------------------------------------------------------------
// 游戏对皮肤透明度的几条规矩。不照做，半透明的像素会被画成玻璃，里层透出来。
// --------------------------------------------------------------------------------------

/**
 * 游戏画皮肤用的是「镂空」：alpha 低于 0.1 的像素丢掉，其余的**完全不透明**地画（不混合）。
 * 所以 alpha ≥ 26 的像素算实心。
 */
export const cutoutAlpha = 26;

/** 里层（头、身体、四肢）的图在游戏里会被强制成不透明：没有透明的头和身体。 */
const baseOpaqueRects = [
  { x: 0, y: 0, w: 32, h: 16 }, // 头
  { x: 0, y: 16, w: 64, h: 16 }, // 右腿、身体、右臂
  { x: 16, y: 48, w: 32, h: 16 }, // 左腿、左臂（新式皮肤）
] as const;

/** `pixels` 的一份拷贝，里层的区域 alpha 都改成 255。外层的区域不动。 */
export function withOpaqueBase(pixels: Uint8ClampedArray, width: number, height: number): Uint8ClampedArray {
  const copy = new Uint8ClampedArray(pixels);
  for (const rect of baseOpaqueRects) {
    for (let y = rect.y; y < rect.y + rect.h && y < height; y += 1) {
      for (let x = rect.x; x < rect.x + rect.w && x < width; x += 1) {
        copy[(y * width + x) * 4 + 3] = 255;
      }
    }
  }
  return copy;
}

/**
 * 旧式（64×32）皮肤的帽子区域是不是整块实心。游戏对这种皮肤有一条老规矩（代码里叫
 * Notch 透明度 hack）：(32,0)–(64,32) 这一块里要是**没有一个**透明像素（alpha < 128），
 * 就当作没有帽子层，整块清成透明。Notch 的皮肤正是这样：这一块是不透明的黑色，不照这条
 * 规矩画，脸会被黑色盖住。`pixels` 是整张皮肤的 RGBA。
 */
export function legacyHatIsSolid(pixels: Uint8ClampedArray, width: number): boolean {
  for (let y = 0; y < 32; y += 1) {
    for (let x = 32; x < 64 && x < width; x += 1) {
      if (pixels[(y * width + x) * 4 + 3] < 128) {
        return false;
      }
    }
  }
  return true;
}

// --------------------------------------------------------------------------------------
// 方块：里层的每个像素和外层的每个体素，都拆成小的四边形，画的时候逐像素比深度
// --------------------------------------------------------------------------------------

/**
 * 一个平行四边形：`corners` 依次是 p0、p0+e1、p0+e1+e2、p0+e2。颜色是红绿蓝（0–255），
 * `normal` 是朝外的单位向量（用来背面剔除和打光）。
 */
export type Quad = {
  corners: readonly [Vec3, Vec3, Vec3, Vec3];
  color: readonly [number, number, number];
  normal: Vec3;
};

/** 体素的一面：沿 `axis`（0=x 1=y 2=z）朝 `sign` 的方向。 */
export type Wall = readonly [axis: 0 | 1 | 2, sign: 1 | -1];

/** 外层的一个像素：一个有厚度的小方块。只记会露出来的面（不被邻居挡住的）。 */
export type Voxel = {
  min: Vec3;
  max: Vec3;
  color: readonly [number, number, number];
  walls: readonly Wall[];
};

function axisOf(vector: Vec3): Wall {
  const axis = vector[0] !== 0 ? 0 : vector[1] !== 0 ? 1 : 2;
  return [axis, vector[axis] > 0 ? 1 : -1];
}

/**
 * 立方体各面的像素，每个实心像素一个四边形（里层用）。`pixels` 要先过 `withOpaqueBase`。
 */
export function boxQuads(box: Box, pixels: Uint8ClampedArray, width: number): Quad[] {
  const quads: Quad[] = [];
  for (const face of faceList) {
    const region = boxRegion(box, face);
    const frame = boxFrame(box, face);
    const normal = faceNormal(face);
    const at = (i: number, j: number): Vec3 => [
      frame.origin[0] + frame.u[0] * i + frame.v[0] * j,
      frame.origin[1] + frame.u[1] * i + frame.v[1] * j,
      frame.origin[2] + frame.u[2] * i + frame.v[2] * j,
    ];
    for (let j = 0; j < region.sh; j += 1) {
      for (let i = 0; i < region.sw; i += 1) {
        const index = ((region.sy + j) * width + region.sx + i) * 4;
        if (pixels[index + 3] < cutoutAlpha) {
          continue;
        }
        quads.push({
          corners: [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)],
          color: [pixels[index], pixels[index + 1], pixels[index + 2]],
          normal,
        });
      }
    }
  }
  return quads;
}

/**
 * 一个外层立方体的体素：展开图上每个实心（alpha ≥ `cutoutAlpha`）的像素一个，从里层的表面
 * 向外凸 `thickness`。在立方体边缘的像素还要向边外多伸 `thickness`，这样相邻两个面的体素
 * 在棱上接得上，不会留缺口。`pixels` 是整张皮肤的 RGBA（外层的区域保持原样，不要过
 * `withOpaqueBase`），`width` 是它的宽。
 */
export function outerVoxels(box: Box, thickness: number, pixels: Uint8ClampedArray, width: number): Voxel[] {
  const voxels: Voxel[] = [];
  for (const face of faceList) {
    const region = boxRegion(box, face);
    const frame = boxFrame(box, face);
    const normal = faceNormal(face);
    const solid = (i: number, j: number): boolean =>
      i >= 0 &&
      j >= 0 &&
      i < region.sw &&
      j < region.sh &&
      pixels[((region.sy + j) * width + region.sx + i) * 4 + 3] >= cutoutAlpha;
    const uWall = axisOf(frame.u);
    const vWall = axisOf(frame.v);
    const outward = axisOf(normal);

    for (let j = 0; j < region.sh; j += 1) {
      for (let i = 0; i < region.sw; i += 1) {
        if (!solid(i, j)) {
          continue;
        }
        const at = (a: number, b: number, extraU: number, extraV: number): Vec3 => [
          frame.origin[0] + frame.u[0] * (i + a + extraU) + frame.v[0] * (j + b + extraV),
          frame.origin[1] + frame.u[1] * (i + a + extraU) + frame.v[1] * (j + b + extraV),
          frame.origin[2] + frame.u[2] * (i + a + extraU) + frame.v[2] * (j + b + extraV),
        ];
        // 角在表面上：边缘的像素向边外多伸一截
        const lo = at(0, 0, i === 0 ? -thickness : 0, j === 0 ? -thickness : 0);
        const hi = at(1, 1, i === region.sw - 1 ? thickness : 0, j === region.sh - 1 ? thickness : 0);
        const points: Vec3[] = [
          lo,
          hi,
          [lo[0] + normal[0] * thickness, lo[1] + normal[1] * thickness, lo[2] + normal[2] * thickness],
          [hi[0] + normal[0] * thickness, hi[1] + normal[1] * thickness, hi[2] + normal[2] * thickness],
        ];
        const min: Vec3 = [
          Math.min(...points.map((p) => p[0])),
          Math.min(...points.map((p) => p[1])),
          Math.min(...points.map((p) => p[2])),
        ];
        const max: Vec3 = [
          Math.max(...points.map((p) => p[0])),
          Math.max(...points.map((p) => p[1])),
          Math.max(...points.map((p) => p[2])),
        ];

        // 露出来的面：朝外的一面一定有；四周的侧面只在邻居不是实心的（或到了边）时才有
        const walls: Wall[] = [outward];
        const side = (neighbourSolid: boolean, wall: Wall, flip: 1 | -1) => {
          if (!neighbourSolid) {
            walls.push([wall[0], (wall[1] * flip) as 1 | -1]);
          }
        };
        side(solid(i - 1, j), uWall, -1);
        side(solid(i + 1, j), uWall, 1);
        side(solid(i, j - 1), vWall, -1);
        side(solid(i, j + 1), vWall, 1);

        const index = ((region.sy + j) * width + region.sx + i) * 4;
        voxels.push({ min, max, color: [pixels[index], pixels[index + 1], pixels[index + 2]], walls });
      }
    }
  }
  return voxels;
}

/** 一个体素露出来的各面，每面一个四边形。 */
export function voxelQuads(voxel: Voxel): Quad[] {
  return voxel.walls.map(([axis, sign]): Quad => {
    const plane = sign > 0 ? voxel.max[axis] : voxel.min[axis];
    const b = (axis + 1) % 3;
    const c = (axis + 2) % 3;
    const corner = (vb: number, vc: number): Vec3 => {
      const p: [number, number, number] = [0, 0, 0];
      p[axis] = plane;
      p[b] = vb;
      p[c] = vc;
      return p;
    };
    const normal: [number, number, number] = [0, 0, 0];
    normal[axis] = sign;
    return {
      corners: [
        corner(voxel.min[b], voxel.min[c]),
        corner(voxel.max[b], voxel.min[c]),
        corner(voxel.max[b], voxel.max[c]),
        corner(voxel.min[b], voxel.max[c]),
      ],
      color: voxel.color,
      normal,
    };
  });
}

// --------------------------------------------------------------------------------------
// 旋转：3×3 矩阵（按行排），可以绕三个轴任意转
// --------------------------------------------------------------------------------------

export type Mat3 = readonly [number, number, number, number, number, number, number, number, number];

export const identity: Mat3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

/** `a · b`：先做 b，再做 a。 */
export function matMul(a: Mat3, b: Mat3): Mat3 {
  const r = (row: number, col: number) =>
    a[row * 3] * b[col] + a[row * 3 + 1] * b[3 + col] + a[row * 3 + 2] * b[6 + col];
  return [r(0, 0), r(0, 1), r(0, 2), r(1, 0), r(1, 1), r(1, 2), r(2, 0), r(2, 1), r(2, 2)];
}

/** 绕 x 轴转：正的是从上往下看（顶面朝向观察者）。 */
export function rotationX(angle: number): Mat3 {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [1, 0, 0, 0, c, -s, 0, s, c];
}

/** 绕 y 轴转：正的是人物转向它自己的右边（能看到它右侧）。 */
export function rotationY(angle: number): Mat3 {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [c, 0, s, 0, 1, 0, -s, 0, c];
}

/** 绕 z 轴（朝向观察者的轴）转：正的是逆时针翻滚。 */
export function rotationZ(angle: number): Mat3 {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [c, -s, 0, s, c, 0, 0, 0, 1];
}

/** 转 `yaw`（绕 y）、`pitch`（绕 x）、`roll`（绕 z），顺序是先 roll、再 yaw、最后 pitch。 */
export function viewFromAngles(yaw: number, pitch: number, roll = 0): Mat3 {
  return matMul(rotationX(pitch), matMul(rotationY(yaw), rotationZ(roll)));
}

/** 把空间里的向量转到观察者的坐标（z 越大离观察者越近）。 */
export function transform(m: Mat3, p: Vec3): Vec3 {
  return [
    m[0] * p[0] + m[1] * p[1] + m[2] * p[2],
    m[3] * p[0] + m[4] * p[1] + m[5] * p[2],
    m[6] * p[0] + m[7] * p[1] + m[8] * p[2],
  ];
}

/**
 * 一次次叠加的小旋转会让矩阵慢慢变形（不再是纯旋转）。把它的三行重新拉成互相垂直的单位
 * 向量（Gram–Schmidt）。
 */
export function orthonormalize(m: Mat3): Mat3 {
  const norm = (v: Vec3): Vec3 => {
    const length = Math.hypot(v[0], v[1], v[2]) || 1;
    return [v[0] / length, v[1] / length, v[2] / length];
  };
  const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const r0 = norm([m[0], m[1], m[2]]);
  const raw1: Vec3 = [m[3], m[4], m[5]];
  const d = dot(raw1, r0);
  const r1 = norm([raw1[0] - r0[0] * d, raw1[1] - r0[1] * d, raw1[2] - r0[2] * d]);
  const r2: Vec3 = [
    r0[1] * r1[2] - r0[2] * r1[1],
    r0[2] * r1[0] - r0[0] * r1[2],
    r0[0] * r1[1] - r0[1] * r1[0],
  ];
  return [r0[0], r0[1], r0[2], r1[0], r1[1], r1[2], r2[0], r2[1], r2[2]];
}
