import { Text } from "@fluentui/react-components";
import { useRef, useState } from "react";
import type { CSSProperties, PointerEvent } from "react";
import type { SystemMemory } from "../ipc";
import { exceedsAvailable, formatMiB, memorySliderStep, minMemoryMiB, sliderMaxMiB } from "../lib/memory";
import {
  barGeometry,
  thumbForTrackPress,
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

/**
 * 一体化的内存滑块：整条轨道就是本机的物理内存。
 *
 * 左边红色是系统已经用掉的，不能拖；分界线右边是可用内存，右手柄在里面滑动，
 * 绿色从分界线铺到手柄，就是分配给游戏的量。右手柄推到底（「满」）之后，
 * 分界线上出现第二个手柄，可以继续往左拖进红区——那一段变橙色，表示分配得
 * 比可用内存还多，会把内存挤爆。两个手柄控制同一个值。
 *
 * 屏幕上的手柄用指针拖动；真正的 `<input type="range">` 在旁边，只是看不见：
 * 键盘方向键、读屏和表单语义都由它承担，值和手柄始终一致。
 */
export function MemoryBar({ system, valueMiB, onChange, disabled = false }: MemoryBarProps) {
  const trackRef = useRef<HTMLDivElement>(null);
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

  const apply = (thumb: Thumb, pct: number) => {
    onChange?.(thumb === "right" ? valueFromRightThumb(pct, system) : valueFromLeftThumb(pct, system));
  };

  const handleDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!interactive) {
      return;
    }
    const pct = pressedPercent(event);
    if (pct === null) {
      return;
    }
    const target = event.target as HTMLElement;
    const thumb: Thumb | null = target.closest(".memory-thumb.left")
      ? "left"
      : target.closest(".memory-thumb.right")
        ? "right"
        : thumbForTrackPress(pct, geometry);
    if (thumb === null) {
      return;
    }
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDragging(thumb);
    apply(thumb, pct);
  };

  const handleMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragging === null) {
      return;
    }
    const pct = pressedPercent(event);
    if (pct !== null) {
      apply(dragging, pct);
    }
  };

  const stopDragging = () => setDragging(null);

  return (
    <div className="memory-bar" style={style} data-over={over === true} data-disabled={!interactive}>
      <div
        ref={trackRef}
        className="memory-track"
        onPointerDown={handleDown}
        onPointerMove={handleMove}
        onPointerUp={stopDragging}
        onPointerCancel={stopDragging}
      >
        {geometry.usedMib !== null ? (
          <>
            <span className="memory-seg used" />
            <span className="memory-divider" />
          </>
        ) : null}
        <span className="memory-seg alloc" />
        {geometry.leftPct !== null && geometry.overMiB > 0 ? <span className="memory-seg over" /> : null}

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
        {over === true ? (
          <li>
            <span className="swatch over" aria-hidden="true" />
            超出可用内存
          </li>
        ) : null}
      </ul>

      {geometry.usedMib !== null && interactive && geometry.leftPct === null ? (
        <Text size={200} className="muted">
          红色是系统已经占用的内存，不能拖。把右边的手柄拖到底，就可以继续往左分配更多。
        </Text>
      ) : null}
      {over === true ? (
        <Text size={200} className="memory-warning" role="status">
          分配的内存超过了当前可用内存，游戏可能变慢甚至崩溃。可以先关闭一些程序，或调低这个值。
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
