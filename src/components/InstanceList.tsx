import { Badge, Text, Tooltip } from "@fluentui/react-components";
import type { InstanceSummary } from "../ipc";
import { describeInstanceState } from "../lib/instances";
import { formatMemoryMiB } from "../lib/memory";

type InstanceListProps = {
  instances: readonly InstanceSummary[];
  /** 默认内存上限，来自设置；实例行上用同一份值做提示 */
  memoryMiB: number;
  /** 默认 Java 的可读标签，例如 `Java 21 · x64`；没有检测到时为 null */
  javaLabel: string | null;
};

/**
 * 实例列表：只负责展示、搜索过滤后的选中列表，不承载生命周期操作——
 * 启动/停止的唯一入口是列表下方的 `InstanceLaunchPanel`。
 *
 * 用 `<ul>` 而不是表格：一行里既有徽章又有次要信息，语义上是「卡片列表」，
 * 窄窗口下也只需要改 `grid-template-columns`。
 */
export function InstanceList({ instances, memoryMiB, javaLabel }: InstanceListProps) {
  return (
    <ul className="instance-list">
      {instances.map((instance) => {
        const status = describeInstanceState(instance.state);

        return (
          <li className="instance-row" key={instance.id}>
            <span className="instance-badge" aria-hidden="true">
              {instance.version.slice(0, 6)}
            </span>

            <div className="instance-primary">
              <Text weight="semibold" className="instance-name">
                {instance.name}
              </Text>
              <Text className="muted instance-meta">
                {instance.version}
                <span className="meta-separator" aria-hidden="true">
                  ·
                </span>
                {javaLabel ?? "Java 未检测"}
                <span className="meta-separator" aria-hidden="true">
                  ·
                </span>
                {formatMemoryMiB(memoryMiB)}
              </Text>
            </div>

            <Tooltip content={status.hint} relationship="description">
              <Badge appearance="tint" color={status.tone} className="status-chip">
                {status.label}
              </Badge>
            </Tooltip>
          </li>
        );
      })}
    </ul>
  );
}
