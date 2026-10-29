import { Text } from "@fluentui/react-components";
import { useRef, useState } from "react";
import type { CSSProperties, DragEvent, PointerEvent } from "react";
import type { SystemMemory } from "../ipc";
import {
  exceedsAvailable,
  formatMiB,
  memorySliderStep,
  minMemoryMiB,
  sliderMaxMiB,
} from "../lib/memory";
import { barGeometry, memoryTicks, valueFromDrag, valueFromPosition } from "../lib/memoryBar";

type MemoryBarProps = {
  system: SystemMemory | null;
  valueMiB: number;
  /** 不传或 disabled 时只读，用于自动模式展示推荐值 */
  onChange?: (mib: number) => void;
  disabled?: boolean;
};

/** 拖动手柄时记下按下时的指针位置和分配值。 */
type Drag = { startX: number; startValue: number };

/**
 * 内存滑块：一条分配值轴，一个手柄。
 *
 * 轨道从头（左）到尾（右）是 `0 MiB` 到这台机器的物理内存，手柄就是分配值。
 * 往右拖永远是多分配、往左拖永远是少分配，和任何一个普通滑块一样。
 *
 * - **蓝色**从最左端铺到手柄，是分给游戏的那一段；
 * - **红色**从最右端往左铺，是系统已经占掉的。
 *
 * 两段各从一头长出来，互不覆盖。手柄还没碰到红区时中间是空档（可用内存），
 * 一旦压上去，红区立刻贴在手柄右边并整条变橙，标「不推荐」。
 *
 * 之前的版本把红区画在左边、又给越界单独配了一个往左跑的手柄，于是「多分配」
 * 在轨道上有两个相反的方向。现在只有一个手柄。
 *
 * 拖动默认吸附到 512 MiB 的整数倍；按住 Shift 则不吸附（32 MiB 的细粒度）。
 *
 * 屏幕上的手柄用指针拖动；真正的 `<input type="range">` 藏在旁边，键盘方向键、
 * 读屏和表单语义都由它承担，值和手柄始终一致。
 */
export function MemoryBar({ system, valueMiB, onChange, disabled = false }: MemoryBarProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const drag = useRef<Drag | null>(null);
  const [dragging, setDragging] = useState(false);
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
    // `--pos` 不能叫 `--thumb`：那是圆钮的像素尺寸，重名会把尺寸覆盖掉。
    "--pos": geometry.thumbPct,
    "--used-start": geometry.usedStartPct,
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
    event.currentTarget.setPointerCapture?.(event.pointerId);

    const onThumb = (event.target as HTMLElement).closest(".memory-thumb") !== null;
    // 抓住手柄：什么都不变，之后相对于这里拖。直接点在轨道上：先把手柄放到那里，再从那里接着拖
    let startValue = geometry.valueMiB;
    if (!onThumb) {
      startValue = valueFromPosition(pct, system, event.shiftKey);
      onChange?.(startValue);
    }
    drag.current = { startX: event.clientX, startValue };
    setDragging(true);
  };

  const handleMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    const width = trackRef.current?.getBoundingClientRect().width ?? 0;
    if (current === null || width <= 0) {
      return;
    }
    // 不夹在轨道里：指针被捕获，拖到轨道外面也要继续算，这样能一路推到底
    const movedPct = ((event.clientX - current.startX) / width) * 100;
    onChange?.(valueFromDrag(current.startValue, movedPct, system, event.shiftKey));

    // 到了最小值或最大值之后，指针继续走多远都不该欠账：把锚点重置在这个极限上，
    // 指针一掉头手柄就立刻跟着走，而不是先把多走的那一段原路退回来
    const raw = current.startValue + (movedPct / 100) * geometry.totalMib;
    if (raw > max || raw < minMemoryMiB) {
      current.startValue = raw > max ? max : minMemoryMiB;
      current.startX = event.clientX;
    }
  };

  const stopDragging = () => {
    drag.current = null;
    setDragging(false);
  };

  const cancelNativeDrag = (event: DragEvent) => event.preventDefault();

  return (
    <div
      className="memory-bar"
      style={style}
      data-over={over === true}
      data-disabled={!interactive}
      data-dragging={dragging}
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
        {/* 红色从右端往左铺；越界时它会被手柄压住并整条变色。 */}
        {geometry.usedMib !== null ? <span className="memory-seg used" /> : null}
        {/* 蓝色从最左端铺到手柄。 */}
        <span className="memory-seg alloc" />

        {ticks.map((tick) => (
          <span
            key={tick.valueMiB}
            className={`memory-tick${tick.major ? " major" : ""}`}
            style={{ left: `${tick.pct}%` }}
            aria-hidden="true"
          />
        ))}

        <span className={`memory-thumb${dragging ? " dragging" : ""}`} aria-hidden="true" />

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

      <div className="memory-bar-labels">
        <div className="memory-bar-stat">
          <Text size={200} className="muted">
            系统已用 / 总内存
          </Text>
          <Text weight="semibold">
            {geometry.usedMib === null ? "—" : formatMiB(geometry.usedMib)} /{" "}
            {formatMiB(geometry.totalMib)}
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
      </ul>

      {geometry.over ? (
        <Text size={200} className="memory-warning" role="status">
          不推荐：分配的内存超过了当前可用内存，游戏可能变慢甚至崩溃。可以先关闭一些程序，或调低这个值。
        </Text>
      ) : geometry.usedMib !== null && interactive ? (
        <Text size={200} className="muted">
          轨道从左到右是这台机器的物理内存，往右拖就是给游戏多分配。右边的红色是系统已经占掉的，
          手柄碰到红区就说明分配得比可用内存还多。拖动时吸附到 512 MiB，按住 Shift 可以关闭吸附。
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
