import { Radio, RadioGroup, Text } from "@fluentui/react-components";
import { useState } from "react";
import { getSystemMemory } from "../ipc";
import {
  clampMemoryMiB,
  formatMiB,
  isMemoryMode,
  recommendedMemoryMiB,
} from "../lib/memory";
import type { MemoryMode } from "../lib/memory";
import { useQuery } from "../lib/query";
import {
  loadMemoryMiB,
  loadMemoryMode,
  saveMemoryMiB,
  saveMemoryMode,
} from "../lib/settings";
import { MemoryBar } from "./MemoryBar";

/**
 * 「内存」设置区块，这是内存上限唯一的调整入口——创建、安装和启动实例时都不再询问。
 *
 * 滑块的范围来自本机实际安装的内存，而不是一个固定数字。
 * 「自动」按本机当前的可用内存推荐，每次启动重新计算；「自定义」用拖出来的值。
 */
export function MemorySection() {
  const [mode, setMode] = useState<MemoryMode>(loadMemoryMode);
  const [custom, setCustom] = useState(loadMemoryMiB);
  const { data: system } = useQuery(getSystemMemory);

  const recommended = recommendedMemoryMiB(system);

  const changeMode = (next: MemoryMode) => {
    setMode(next);
    saveMemoryMode(next);
  };

  const changeCustom = (mib: number) => {
    setCustom(saveMemoryMiB(clampMemoryMiB(mib)));
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
            分配给游戏的堆内存上限（-Xmx），所有实例共用。滑块的范围就是这台电脑实际安装的内存。
          </Text>
        </div>

        <RadioGroup
          aria-label="内存分配方式"
          value={mode}
          onChange={(_, data) => {
            if (isMemoryMode(data.value)) {
              changeMode(data.value);
            }
          }}
        >
          <Radio value="auto" label={`自动（推荐，现在约 ${formatMiB(recommended)}）`} />
          <Radio value="custom" label="自定义" />
        </RadioGroup>

        <MemoryBar
          system={system}
          valueMiB={mode === "auto" ? recommended : custom}
          onChange={mode === "custom" ? changeCustom : undefined}
        />

        {mode === "auto" ? (
          <Text size={200} className="muted">
            自动模式每次启动游戏时，根据当时的可用内存重新计算：可用内存的 65%，不超过 8 GiB，
            也不超过总内存的 75%。
          </Text>
        ) : null}
      </div>
    </section>
  );
}
