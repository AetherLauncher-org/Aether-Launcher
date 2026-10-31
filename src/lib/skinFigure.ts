/**
 * 把皮肤画成正对着看的整个人：头、身体、两只手臂、两条腿，每个部位先画里层再画外层
 * （帽子、外套、袖子、裤腿）。
 *
 * 皮肤是 64×64 的图；旧的 64×32 的图只有右手右腿，左边的是右边的镜像，没有外层（除了帽子）。
 * 经典手臂 4 像素宽，纤细手臂 3 像素宽。人是 16×32 个像素点，每个点放大到整数倍，最近邻取样。
 */

export type FigurePart = {
  /** 在皮肤图里的来源方块 */
  sx: number;
  sy: number;
  sw: number;
  sh: number;
  /** 在 16×32 的人里的位置 */
  dx: number;
  dy: number;
  /** 左右翻转（旧皮肤的左臂左腿是右边的镜像） */
  flip: boolean;
  overlay: boolean;
};

/** 人的大小，以皮肤的像素点计。 */
export const figureSize = { width: 16, height: 32 } as const;

/**
 * 要画的部位，按先后顺序。`modern` 是 64×64（有独立的左手左腿和外层），`slim` 是纤细手臂。
 */
export function figureParts(modern: boolean, slim: boolean): FigurePart[] {
  const arm = slim ? 3 : 4;
  const part = (
    sx: number,
    sy: number,
    sw: number,
    sh: number,
    dx: number,
    dy: number,
    overlay: boolean,
    flip = false,
  ): FigurePart => ({ sx, sy, sw, sh, dx, dy, flip, overlay });

  const parts: FigurePart[] = [
    // 里层：右腿、左腿、身体、右臂、左臂、头
    part(4, 20, 4, 12, 4, 20, false),
    modern ? part(20, 52, 4, 12, 8, 20, false) : part(4, 20, 4, 12, 8, 20, false, true),
    part(20, 20, 8, 12, 4, 8, false),
    part(44, 20, arm, 12, 4 - arm, 8, false),
    modern ? part(36, 52, arm, 12, 12, 8, false) : part(44, 20, arm, 12, 12, 8, false, true),
    part(8, 8, 8, 8, 4, 0, false),
  ];
  if (modern) {
    // 外层：裤腿、外套、袖子
    parts.push(
      part(4, 36, 4, 12, 4, 20, true),
      part(4, 52, 4, 12, 8, 20, true),
      part(20, 36, 8, 12, 4, 8, true),
      part(44, 36, arm, 12, 4 - arm, 8, true),
      part(52, 52, arm, 12, 12, 8, true),
    );
  }
  // 帽子层两种皮肤都有
  parts.push(part(40, 8, 8, 8, 4, 0, true));
  return parts;
}

/** `width`×`height` 的皮肤图是不是 64×64 的新式皮肤（旧式是 64×32）。 */
export function isModernSkin(width: number, height: number): boolean {
  return width === 64 && height === 64;
}

/** 把 `skin` 画成人，铺在 `target` 上（先清空，画布宽高要是 16×32 的整数倍）。画不了返回 false。 */
export function drawFigure(
  target: HTMLCanvasElement,
  skin: CanvasImageSource & { width: number; height: number },
  slim: boolean,
): boolean {
  const context = target.getContext("2d");
  if (context === null) {
    return false;
  }
  const scale = Math.max(1, Math.floor(target.width / figureSize.width));
  context.clearRect(0, 0, target.width, target.height);
  context.imageSmoothingEnabled = false;
  for (const part of figureParts(isModernSkin(skin.width, skin.height), slim)) {
    const x = part.dx * scale;
    const y = part.dy * scale;
    const w = part.sw * scale;
    const h = part.sh * scale;
    if (part.flip) {
      context.save();
      context.translate(x + w, y);
      context.scale(-1, 1);
      context.drawImage(skin, part.sx, part.sy, part.sw, part.sh, 0, 0, w, h);
      context.restore();
    } else {
      context.drawImage(skin, part.sx, part.sy, part.sw, part.sh, x, y, w, h);
    }
  }
  return true;
}
