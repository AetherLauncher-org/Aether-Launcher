import {
  Badge,
  Button,
  SearchBox,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add24Regular,
  ArrowSync24Regular,
  BoxMultiple24Regular,
  Warning24Regular,
} from "@fluentui/react-icons";
import { useCallback, useMemo, useState } from "react";
import { CreateInstanceDialog } from "../components/CreateInstanceDialog";
import { EmptyState } from "../components/EmptyState";
import { InstanceList } from "../components/InstanceList";
import {
  DesktopOnlyNotice,
  ErrorState,
  ListSkeleton,
} from "../components/PageState";
import { PageHeader } from "../components/PageHeader";
import { emptyInstanceListing, isTauri, listInstances } from "../ipc";
import type { InstanceListing, InstanceSummary } from "../ipc";
import {
  describeSkippedReason,
  filterInstances,
  sortInstances,
  summariseInstances,
} from "../lib/instances";
import { formatMemoryMiB } from "../lib/memory";
import { useQuery } from "../lib/query";
import { loadJavaRuntimeKey, loadMemoryMiB } from "../lib/settings";
import type { PageDefinition } from "../navigation";

type InstancesPageProps = {
  page: PageDefinition;
};

/**
 * 实例页。
 *
 * 数据来自 `list_instances`；浏览器预览里没有 Tauri，返回空列表并显示
 * 「需要桌面版」的提示，不把环境差异当成错误。
 */
export function InstancesPage({ page }: InstancesPageProps) {
  const desktop = isTauri();
  const [query, setQuery] = useState("");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [listing, setListing] = useState<InstanceListing>(emptyInstanceListing);
  const [memoryMiB] = useState(loadMemoryMiB);
  const [javaKey] = useState(loadJavaRuntimeKey);

  const load = useCallback(async () => {
    const result = await listInstances();
    setListing(result);
    return result;
  }, []);

  const { status, error, reload } = useQuery(load, desktop);

  // 浏览器预览下 useQuery 不运行，直接用空列表展示
  const instances = useMemo(
    () => (desktop ? listing.instances : []),
    [desktop, listing.instances],
  );
  const skipped = useMemo(
    () => (desktop ? listing.skipped : []),
    [desktop, listing.skipped],
  );

  const visible = useMemo(
    () => sortInstances(filterInstances(instances, query)),
    [instances, query],
  );

  const handleCreated = useCallback(
    (created: InstanceSummary) => {
      setListing((previous) => ({
        ...previous,
        instances: [...previous.instances, created],
      }));
    },
    [],
  );

  // 徽章只报数量，空状态标题才负责「还没有实例」这句话，避免同一句文案出现两次
  const badgeText = instances.length === 0 ? "0 个实例" : summariseInstances(instances);

  return (
    <>
      <PageHeader
        title={page.label}
        description={page.summary}
        action={
          <Button
            appearance="primary"
            icon={<Add24Regular />}
            disabled={!desktop}
            onClick={() => setDialogOpen(true)}
          >
            创建实例
          </Button>
        }
      />

      {!desktop ? (
        <DesktopOnlyNotice body="浏览器预览读不到本机实例。用桌面版可以扫描、创建和启动实例。" />
      ) : null}

      <section className="status-panel list-panel" aria-labelledby="instances-heading">
        <div className="list-toolbar">
          <div className="toolbar-heading">
            <Text id="instances-heading" as="h2" size={400} weight="semibold">
              本机实例
            </Text>
            <Badge appearance="outline">{badgeText}</Badge>
          </div>

          <div className="toolbar-actions">
            <SearchBox
              className="filter-field"
              placeholder="按名称或版本筛选"
              aria-label="筛选实例"
              value={query}
              onChange={(_, data) => setQuery(data.value)}
            />
            <Tooltip
              content="导入现有 .minecraft 或其他启动器的实例还在开发，完成后这里可以一键导入"
              relationship="description"
            >
              <Button icon={<Add24Regular />} disabled aria-label="导入实例">
                导入
              </Button>
            </Tooltip>
            <Button
              icon={<ArrowSync24Regular />}
              onClick={reload}
              disabled={!desktop || status === "loading"}
            >
              刷新
            </Button>
          </div>
        </div>

        {status === "error" && error ? (
          <ErrorState
            title="读取实例失败"
            message={error}
            onRetry={reload}
            retryLabel="重新读取"
          />
        ) : status === "loading" && instances.length === 0 ? (
          <ListSkeleton rows={3} />
        ) : instances.length === 0 ? (
          <EmptyState
            icon={<BoxMultiple24Regular />}
            title="还没有实例"
            body={
              desktop
                ? "创建一个实例开始，或者等导入功能上线后从现有启动器导入。"
                : "浏览器预览里无法读取本机实例，打开桌面版即可看到列表。"
            }
            action={
              <Button
                appearance="primary"
                icon={<Add24Regular />}
                disabled={!desktop}
                onClick={() => setDialogOpen(true)}
              >
                创建实例
              </Button>
            }
          />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={<Warning24Regular />}
            title="没有匹配的实例"
            body={`没有名称或版本包含「${query.trim()}」的实例，换个关键词试试。`}
          />
        ) : (
          <InstanceList
            instances={visible}
            memoryMiB={memoryMiB}
            javaLabel={javaKey === null ? null : formatJavaLabel(javaKey)}
          />
        )}

        {skipped.length > 0 ? (
          <div className="skipped-block">
            <Text weight="semibold">有 {skipped.length} 条记录没读出来</Text>
            <ul className="skipped-list">
              {skipped.map((item) => (
                <li key={item.id}>
                  <Text className="muted">
                    {item.id}：{describeSkippedReason(item.reason)}
                  </Text>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      <CreateInstanceDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onCreated={handleCreated}
      />

      <Text className="page-footnote muted">
        默认内存 {formatMemoryMiB(memoryMiB)}，可在设置里调整。
      </Text>
    </>
  );
}

/** `localStorage` 里存的是完整路径，这里只取文件名所在的最后一段。 */
function formatJavaLabel(key: string): string {
  const parts = key.split(/[\\/]/).filter((part) => part.length > 0);
  return parts.length > 0 ? parts[parts.length - 1] : key;
}
