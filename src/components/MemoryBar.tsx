import { Text } from "@fluentui/react-components";
import { useRef, useState } from "react";
import type { CSSProperties, DragEvent, PointerEvent } from "react";
import type { SystemMemory } from "../ipc";
import { exceedsAvailable, formatMiB, memorySliderStep, minMemoryMiB, sliderMaxMiB } from "../lib/memory";
import {
  barGeometry,
  memoryTicks,
  thumbForTrackPress,
  valueFromDrag,
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

/** 两个手柄都是相对拖动的：记下按下时的指针位置和分配值。 */
type Drag = { thumb: Thumb; startX: number; startValue: number };

/**
 * 一体化的内存滑块：整条轨道就是本机的物理内存。
 *
 * 左边红色是系统已经用掉的，不能拖；分界线右边是可用内存，蓝色手柄在里面滑动，
 * 蓝色从分界线铺到手柄，就是分配给游戏的量。橙色手柄一直停在分界线上。
 * 两个手柄是同一种相对的控制：蓝色往右、橙色往左都是「多分配」，反之是「少分配」。
 * 蓝色没到底时多出来的部分让蓝色往右增加；蓝色到底之后继续多分配（往右推蓝色，或
 * 往左拖橙色），橙色才真的往左走进红区，那一段是橙色，**不推荐**，因为分配得比可用
 * 内存还多，会把内存挤爆。反方向则是橙色先退回分界线，再往下蓝色才减少。
 *
 * 拖动默认吸附到 512 MiB 的整数倍；按住 Shift 拖动则不吸附（32 MiB 的细粒度）。
 *
 * 屏幕上的手柄用指针拖动；真正的 `<input type="range">` 在旁边，只是看不见：
 * 键盘方向键、读屏和表单语义都由它承担，值和手柄始终一致。
 */
export function MemoryBar({ system, valueMiB, onChange, disabled = false }: MemoryBarProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const drag = useRef<Drag | null>(null);
  const [dragging, setDragging] = useState<Thumb | null>(null);
  // 焦点环只在真正用键盘操作时出现：浏览器的 :focus-visible 在鼠标聚焦之后
  // 按一下 Shift 也会亮起来，整条轨道外面突然多一圈白框，很丑
  const [keyboard, setKeyboard] = useState(false);
  const interactive = !disabled && onChange !== undefined;

  const geometry = barGeometry(valueMiB, system);
  const ticks = memoryTicks(system);
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
    setKeyboard(false);
    inputRef.current?.focus({ preventScroll: true });
    const target = event.target as HTMLElement;
    const onLeftThumb = target.closest(".memory-thumb.left") !== null;
    const onRightThumb = target.closest(".memory-thumb.right") !== null;
    const thumb: Thumb = onLeftThumb ? "left" : onRightThumb ? "right" : thumbForTrackPress(pct, geometry);

    event.currentTarget.setPointerCapture?.(event.pointerId);

    // 抓住手柄：什么都不变，之后相对于这里拖。直接点在轨道上：先把手柄放到那里，再从那里接着拖
    let startValue = geometry.valueMiB;
    if (!onLeftThumb && !onRightThumb) {
      startValue =
        thumb === "right"
          ? valueFromRightThumb(pct, system, event.shiftKey)
          : valueFromLeftThumb(pct, system, event.shiftKey);
      onChange?.(startValue);
    }
    drag.current = { thumb, startX: event.clientX, startValue };
    setDragging(thumb);
  };

  const handleMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    const width = trackRef.current?.getBoundingClientRect().width ?? 0;
    if (current === null || width <= 0) {
      return;
    }
    // 不夹在轨道里：指针被捕获，拖到轨道外面也要继续算，这样把蓝色一路往右推也能推过「满」
    const movedPct = ((event.clientX - current.startX) / width) * 100;
    const increasePct = current.thumb === "right" ? movedPct : -movedPct;
    onChange?.(valueFromDrag(current.startValue, increasePct, system, event.shiftKey));

    // 推到最小值或最大值之后，指针继续走多远都不该欠账：把锚点重置在这个极限上，
    // 指针一掉头手柄就立刻跟着走，而不是先把多走的那一段原路退回来
    const raw = current.startValue + (increasePct / 100) * geometry.totalMib;
    if (raw > max || raw < minMemoryMiB) {
      current.startValue = raw > max ? max : minMemoryMiB;
      current.startX = event.clientX;
    }
  };

  const stopDragging = () => {
    drag.current = null;
    setDragging(null);
  };

  const cancelNativeDrag = (event: DragEvent) => event.preventDefault();

  return (
    <div
      className="memory-bar"
      style={style}
      data-over={over === true}
      data-disabled={!interactive}
      data-dragging={dragging !== null}
      data-keyboard={keyboard}
    >
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

        {ticks.map((tick) => (
          <span
            key={tick.valueMiB}
            className={`memory-tick${tick.major ? " major" : ""}`}
            style={{ left: `${tick.pct}%` }}
            aria-hidden="true"
          />
        ))}

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
          onKeyDown={(event) => {
            if (!["Shift", "Control", "Alt", "Meta"].includes(event.key)) {
              setKeyboard(true);
            }
          }}
          onBlur={() => setKeyboard(false)}
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
          蓝色是分配给游戏的内存。往右拖蓝色手柄、或往左拖橙色手柄都是多分配；超过可用内存的部分会变成橙色，不推荐。拖动时吸附到 512 MiB，按住 Shift 可以关闭吸附。
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
