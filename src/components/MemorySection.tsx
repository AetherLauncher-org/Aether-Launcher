import { Input, Slider, Text } from "@fluentui/react-components";
import { useState } from "react";
import {
  clampMemoryMiB,
  formatMemoryMiB,
  memorySliderStep,
} from "../lib/memory";
import { loadMemoryMiB, memoryBounds, saveMemoryMiB } from "../lib/settings";

/**
 * 「内存」设置区块：滑块 + 数字输入，两者共用一个值。这是内存上限唯一的调整入口——
 * 创建、安装和启动实例时都不再询问，直接用这里的值。
 *
 * 输入框允许中间态（空串、输入到一半的数字），失焦或回车时再夹到
 * 512–131072 之间，并写进 `localStorage` 的 `aether.memoryMiB`。
 */
export function MemorySection() {
  const [memory, setMemory] = useState(loadMemoryMiB);
  const [draft, setDraft] = useState(String(loadMemoryMiB()));

  const commit = (next: number) => {
    const clamped = saveMemoryMiB(next);
    setMemory(clamped);
    setDraft(String(clamped));
  };

  const handleSlider = (value: number) => {
    const clamped = clampMemoryMiB(value);
    setMemory(clamped);
    setDraft(String(clamped));
    saveMemoryMiB(clamped);
  };

  return (
    <section className="status-panel setting-group" aria-labelledby="memory-heading">
      <Text id="memory-heading" as="h2" size={400} weight="semibold">
        内存
      </Text>
      <div className="setting-row stacked">
        <div className="setting-copy">
          <Text weight="semibold">游戏内存上限</Text>
          <Text size={200} className="muted">
            分配给游戏的堆内存上限，所有实例共用。范围 {formatMemoryMiB(memoryBounds.min)}–
            {formatMemoryMiB(memoryBounds.max)}，默认 {formatMemoryMiB(memoryBounds.defaultValue)}。
            单机整合包通常 4–8 GiB 就够。
          </Text>
        </div>

        <div className="memory-slider-row">
          <Slider
            aria-label="内存上限"
            min={memoryBounds.min}
            max={memoryBounds.max}
            step={memorySliderStep}
            value={memory}
            onChange={(_, data) => handleSlider(data.value)}
          />
          <div className="memory-value">
            <Input
              type="number"
              aria-label="内存上限（MiB）"
              value={draft}
              min={memoryBounds.min}
              max={memoryBounds.max}
              step={memorySliderStep}
              onChange={(_, data) => setDraft(data.value)}
              onBlur={() => commit(Number.parseInt(draft, 10))}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  commit(Number.parseInt(draft, 10));
                }
              }}
            />
            <Text size={200} className="muted">
              MiB
            </Text>
          </div>
        </div>

        <Text size={200} className="muted memory-summary">
          当前设置：{formatMemoryMiB(memory)}
        </Text>
      </div>
    </section>
  );
}
