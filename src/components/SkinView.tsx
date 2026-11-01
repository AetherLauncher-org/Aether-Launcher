import { useEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { defaultView, prepareSkin, renderSkin, viewSize } from "../lib/skinRender";
import type { SkinImage } from "../lib/skinRender";
import { matMul, orthonormalize, rotationX, rotationY, rotationZ } from "../lib/skinModel";
import type { Mat3 } from "../lib/skinModel";

type SkinViewProps = {
  /** 已经加载好的皮肤图；还没有时留一个空位 */
  image: SkinImage | null;
  slim: boolean;
  /** 一个皮肤像素点放大几倍，默认 6 */
  scale?: number;
  /** 给了就当成一张图读出来，没给就是装饰（读屏跳过） */
  label?: string;
};

/** 拖动一个屏幕像素转多少弧度。 */
const RADIANS_PER_PIXEL = 0.012;

/**
 * 一张皮肤画成有体积的小人：里层是立方体，外层（帽子、外套、袖子、裤腿）是有厚度的体素。
 * 平时是稍微侧一点的 3/4 角度。**按住鼠标拖动**才转：左右拖是左右转，上下拖是前后翻，
 * 按住 Shift 拖是在平面里翻滚，三个轴都能转；双击回到原来的角度。鼠标只是移过去不会转。
 * 没有 2D 画布（测试环境）或读不出皮肤的像素时什么也不画，不报错。
 */
export function SkinView({ image, slim, scale = 6, label }: SkinViewProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const [view, setView] = useState<Mat3>(defaultView);
  const prepared = useMemo(() => (image === null ? null : prepareSkin(image, slim)), [image, slim]);

  const ratio = typeof window === "undefined" ? 1 : Math.min(2, window.devicePixelRatio || 1);
  const width = viewSize.width * scale;
  const height = viewSize.height * scale;

  useEffect(() => {
    if (prepared !== null && canvas.current !== null) {
      renderSkin(canvas.current, prepared, view);
    }
  }, [prepared, view]);

  const onPointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0) {
      return;
    }
    drag.current = { x: event.clientX, y: event.clientY };
    // 拖到画布外面也继续转（测试环境的 jsdom 没有这个方法）
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };

  const onPointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    const start = drag.current;
    if (start === null) {
      return; // 没按住：只是移过，不转
    }
    const dx = (event.clientX - start.x) * RADIANS_PER_PIXEL;
    const dy = (event.clientY - start.y) * RADIANS_PER_PIXEL;
    drag.current = { x: event.clientX, y: event.clientY };
    // 转动是相对屏幕的：新的转动叠在现在的上面（先转现在的，再按屏幕的轴转一下）
    const step = event.shiftKey ? rotationZ(-dx) : matMul(rotationX(dy), rotationY(dx));
    setView((current) => orthonormalize(matMul(step, current)));
  };

  const endDrag = (event: PointerEvent<HTMLCanvasElement>) => {
    drag.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <canvas
      ref={canvas}
      className="skin-figure"
      width={Math.round(width * ratio)}
      height={Math.round(height * ratio)}
      style={{ width, height }}
      title="按住拖动旋转，Shift+拖动翻滚，双击复位"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={() => setView(defaultView)}
    />
  );
}
