import { Text } from "@fluentui/react-components";
import type { CSSProperties } from "react";
import type { SystemMemory } from "../ipc";
import {
  exceedsAvailable,
  formatMiB,
  memorySliderStep,
  minMemoryMiB,
  sliderMaxMiB,
  usedMiB,
} from "../lib/memory";

type MemoryBarProps = {
  system: SystemMemory | null;
  valueMiB: number;
  /** 不传或 disabled 时只读，用于自动模式展示推荐值 */
  onChange?: (mib: number) => void;
  disabled?: boolean;
};

/**
 * 一体化的内存滑块：整条轨道就是本机的物理内存。
 *
 * - 从左边铺开的是分配给游戏的部分，滑块的手柄就在它的末端；
 * - 从右边铺开的是其他程序已经占用的部分；
 * - 两段在中间相遇就变成警告色——这说明分配的内存已经超过当前能用的。
 *
 * 轨道、已用量和手柄是同一个控件，不是一根进度条加一根滑块。
 */
export function MemoryBar({ system, valueMiB, onChange, disabled = false }: MemoryBarProps) {
  const max = sliderMaxMiB(system);
  const total = system?.totalMib ?? max;
  const used = usedMiB(system);
  const value = Math.min(Math.max(valueMiB, minMemoryMiB), max);
  const over = exceedsAvailable(value, system);

  const percent = (mib: number) => Math.min(100, Math.max(0, (mib / total) * 100));
  const style = {
    "--alloc": percent(value),
    "--used": used === null ? 0 : percent(used),
  } as CSSProperties;

  return (
    <div className="memory-bar" style={style} data-over={over === true}>
      <input
        type="range"
        className="memory-range"
        aria-label="游戏内存上限"
        min={minMemoryMiB}
        max={max}
        step={memorySliderStep}
        value={value}
        disabled={disabled || onChange === undefined}
        aria-valuetext={formatMiB(value)}
        onChange={(event) => onChange?.(Number(event.target.value))}
      />

      <div className="memory-bar-labels">
        <div className="memory-bar-stat">
          <Text size={200} className="muted">
            系统已用 / 总内存
          </Text>
          <Text weight="semibold">
            {used === null ? "—" : formatMiB(used)} / {formatMiB(total)}
          </Text>
        </div>
        <div className="memory-bar-stat end">
          <Text size={200} className="muted">
            游戏分配
          </Text>
          <Text weight="semibold">
            {formatMiB(value)}
            {system?.availableMib != null ? `（可用 ${formatMiB(system.availableMib)}）` : ""}
          </Text>
        </div>
      </div>

      <ul className="memory-legend" aria-label="图例">
        <li>
          <span className="swatch alloc" aria-hidden="true" />
          游戏分配
        </li>
        {used !== null ? (
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
