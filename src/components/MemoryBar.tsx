import { Text } from "@fluentui/react-components";
import { useRef, useState } from "react";
import type { CSSProperties, DragEvent, PointerEvent } from "react";
import type { SystemMemory } from "../ipc";
import { exceedsAvailable, formatMiB, memorySliderStep, minMemoryMiB, sliderMaxMiB } from "../lib/memory";
import {
  barGeometry,
  thumbForTrackPress,
  valueFromLeftDrag,
  valueFromLeftThumb,
  valueFromRightThumb,
} from "../lib/memoryBar";

type MemoryBarProps = {
  system: SystemMemory | null;
  valueMiB: number;
  /** 不传或 disabled 时只读，用于自动模式展示推荐值 */
  onChange?: (mib: number) => void;
  disabled?: boolean;
};

type Thumb = "right" | "left";

/** 橙色手柄是相对拖动的：记下按下时的位置和分配值。 */
type LeftDrag = { startPct: number; startValue: number };

/**
 * 一体化的内存滑块：整条轨道就是本机的物理内存。
 *
 * 左边红色是系统已经用掉的，不能拖；分界线右边是可用内存，蓝色手柄在里面滑动，
 * 蓝色从分界线铺到手柄，就是分配给游戏的量。橙色手柄一直停在分界线上：
 * 往左拖是「多分配」——蓝色没到底时，是蓝色往右增加；蓝色到底之后，橙色才真的往左
 * 走进红区，那一段是橙色，**不推荐**，因为分配得比可用内存还多，会把内存挤爆。
 * 往右拖则是「少分配」：橙色先退回分界线，再往右，蓝色就直接减少。
 * 两个手柄控制同一个值。
 *
 * 屏幕上的手柄用指针拖动；真正的 `<input type="range">` 在旁边，只是看不见：
 * 键盘方向键、读屏和表单语义都由它承担，值和手柄始终一致。
 */
export function MemoryBar({ system, valueMiB, onChange, disabled = false }: MemoryBarProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const leftDrag = useRef<LeftDrag | null>(null);
  const [dragging, setDragging] = useState<Thumb | null>(null);
  const interactive = !disabled && onChange !== undefined;

  const geometry = barGeometry(valueMiB, system);
  const max = sliderMaxMiB(system);
  const over = exceedsAvailable(geometry.valueMiB, system);
  const free = system?.availableMib;

  const style = {
    "--used": geometry.usedPct,
    "--right": geometry.rightPct,
    "--left": geometry.leftPct ?? geometry.usedPct,
  } as CSSProperties;

  const pressedPercent = (event: PointerEvent): number | null => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0) {
      return null;
    }
    return Math.min(100, Math.max(0, ((event.clientX - rect.left) / rect.width) * 100));
  };

  const handleDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!interactive) {
      return;
    }
    const pct = pressedPercent(event);
    if (pct === null) {
      return;
    }
    // 不拦住默认行为，浏览器会把这次按下当成选中文字或拖拽元素的开头：
    // 指针立刻变成「禁止」图标，后面的 pointermove 也就收不到了
    event.preventDefault();
    inputRef.current?.focus({ preventScroll: true });
    const target = event.target as HTMLElement;
    const onLeftThumb = target.closest(".memory-thumb.left") !== null;
    const onRightThumb = target.closest(".memory-thumb.right") !== null;
    const thumb: Thumb = onLeftThumb ? "left" : onRightThumb ? "right" : thumbForTrackPress(pct, geometry);

    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDragging(thumb);

    if (thumb === "right") {
      onChange?.(valueFromRightThumb(pct, system));
      return;
    }
    if (onLeftThumb) {
      // 抓住手柄：什么都不变，之后相对于这里拖
      leftDrag.current = { startPct: pct, startValue: geometry.valueMiB };
      return;
    }
    // 直接点在红区：把橙色手柄放到那里，再从那里接着拖
    const placed = valueFromLeftThumb(pct, system);
    leftDrag.current = { startPct: pct, startValue: placed };
    onChange?.(placed);
  };

  const handleMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragging === null) {
      return;
    }
    const pct = pressedPercent(event);
    if (pct === null) {
      return;
    }
    if (dragging === "right") {
      onChange?.(valueFromRightThumb(pct, system));
    } else if (leftDrag.current !== null) {
      const { startPct, startValue } = leftDrag.current;
      onChange?.(valueFromLeftDrag(startValue, startPct - pct, system));
    }
  };

  const stopDragging = () => {
    leftDrag.current = null;
    setDragging(null);
  };

  const cancelNativeDrag = (event: DragEvent) => event.preventDefault();

  return (
    <div className="memory-bar" style={style} data-over={over === true} data-disabled={!interactive}>
      <div
        ref={trackRef}
        className="memory-track"
        onPointerDown={handleDown}
        onPointerMove={handleMove}
        onPointerUp={stopDragging}
        onPointerCancel={stopDragging}
        onLostPointerCapture={stopDragging}
        onDragStart={cancelNativeDrag}
      >
        {geometry.usedMib !== null ? (
          <>
            <span className="memory-seg used" />
            <span className="memory-divider" />
          </>
        ) : null}
        <span className="memory-seg alloc" />
        {geometry.overMiB > 0 ? <span className="memory-seg over" /> : null}

        <span
          className={`memory-thumb right${dragging === "right" ? " dragging" : ""}`}
          aria-hidden="true"
        />
        {geometry.leftPct !== null ? (
          <span
            className={`memory-thumb left${dragging === "left" ? " dragging" : ""}`}
            aria-hidden="true"
          />
        ) : null}

        <input
          ref={inputRef}
          type="range"
          className="memory-input"
          aria-label="游戏内存上限"
          min={minMemoryMiB}
          max={max}
          step={memorySliderStep}
          value={geometry.valueMiB}
          disabled={!interactive}
          aria-valuetext={formatMiB(geometry.valueMiB)}
          onChange={(event) => onChange?.(Number(event.target.value))}
        />
      </div>

      {geometry.overMiB > 0 ? (
        <div className="memory-over-label" aria-hidden="true">
          <span>不推荐</span>
        </div>
      ) : null}

      <div className="memory-bar-labels">
        <div className="memory-bar-stat">
          <Text size={200} className="muted">
            系统已用 / 总内存
          </Text>
          <Text weight="semibold">
            {geometry.usedMib === null ? "—" : formatMiB(geometry.usedMib)} / {formatMiB(geometry.totalMib)}
          </Text>
        </div>
        <div className="memory-bar-stat end">
          <Text size={200} className="muted">
            游戏分配
          </Text>
          <Text weight="semibold">
            {formatMiB(geometry.valueMiB)}
            {free != null ? `（可用 ${formatMiB(free)}）` : ""}
          </Text>
        </div>
      </div>

      <ul className="memory-legend" aria-label="图例">
        <li>
          <span className="swatch alloc" aria-hidden="true" />
          游戏分配
        </li>
        {geometry.usedMib !== null ? (
          <li>
            <span className="swatch used" aria-hidden="true" />
            系统已用
          </li>
        ) : null}
        {geometry.usedMib !== null ? (
          <li>
            <span className="swatch over" aria-hidden="true" />
            超出可用内存（不推荐）
          </li>
        ) : null}
      </ul>

      {over === true ? (
        <Text size={200} className="memory-warning" role="status">
          不推荐：分配的内存超过了当前可用内存，游戏可能变慢甚至崩溃。可以先关闭一些程序，或调低这个值。
        </Text>
      ) : geometry.usedMib !== null && interactive ? (
        <Text size={200} className="muted">
          蓝色是分配给游戏的内存。往左拖橙色手柄可以多分配；超过可用内存的部分会变成橙色，不推荐。
        </Text>
      ) : null}
      {system === null ? (
        <Text size={200} className="muted">
          没能读取本机内存，范围按 {formatMiB(max)} 估计。
        </Text>
      ) : null}
    </div>
  );
}
