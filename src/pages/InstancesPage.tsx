import {
  Button,
  MessageBar,
  MessageBarActions,
  MessageBarBody,
  SearchBox,
  Text,
} from "@fluentui/react-components";
import {
  ArrowDownload20Regular,
  ArrowImport20Regular,
  ArrowSync20Regular,
  Box24Regular,
  Warning24Regular,
} from "@fluentui/react-icons";
import { useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { EmptyState } from "../components/EmptyState";
import { GameControls } from "../components/GameControls";
import { ImportInstancesDialog } from "../components/ImportInstancesDialog";
import { InstanceCover } from "../components/InstanceCover";
import { PageHeader } from "../components/PageHeader";
import { DesktopOnlyNotice, ErrorState } from "../components/PageState";
import {
  describeSkippedReason,
  filterInstances,
  sortInstances,
  summariseInstances,
} from "../lib/instances";
import { describeMemorySetting } from "../lib/memory";
import { loadMemoryMiB, loadMemoryMode } from "../lib/settings";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";
import { useInstances } from "../state/instances";
import { useNavigation } from "../state/navigation";

type InstancesPageProps = {
  page: PageDefinition;
};

/**
 * 实例页：卡片网格。每张卡片有封面、名称、版本和启动／停止按钮。
 *
 * 实例有两条来源：下载页的安装流程，以及从别的启动器导入。两者都落到同一个
 * 实例存储，所以这里只有一个入口负责展示和启动。
 */
export function InstancesPage({ page }: InstancesPageProps) {
  const { desktop, status, instances, skipped, error, reload } = useInstances();
  const { book } = useAccounts();
  const { navigate } = useNavigation();
  const [query, setQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [memoryLabel] = useState(() => describeMemorySetting(loadMemoryMode(), loadMemoryMiB()));

  const visible = useMemo(
    () => sortInstances(filterInstances(instances, query)),
    [instances, query],
  );

  const actions = (
    <div className="page-actions">
      {desktop ? (
        <Button appearance="subtle" icon={<ArrowImport20Regular />} onClick={() => setImporting(true)}>
          导入实例
        </Button>
      ) : null}
      <Button appearance="primary" icon={<ArrowDownload20Regular />} onClick={() => navigate("download")}>
        安装新版本
      </Button>
    </div>
  );

  return (
    <>
      <PageHeader title={page.label} description={page.summary} action={actions} />

      {!desktop ? (
        <DesktopOnlyNotice body="浏览器预览读不到本机实例。用桌面版可以安装、管理和启动实例。" />
      ) : null}

      {desktop && instances.length > 0 && book.accounts.length === 0 ? (
        <MessageBar intent="info" className="page-notice">
          <MessageBarBody>还没有账户，添加离线账户或登录微软账户后才能启动游戏。</MessageBarBody>
          <MessageBarActions>
            <Button size="small" onClick={() => navigate("accounts")}>
              去添加
            </Button>
          </MessageBarActions>
        </MessageBar>
      ) : null}

      {instances.length > 0 ? (
        <div className="toolbar">
          <Text className="muted">{summariseInstances(instances)}</Text>
          <div className="toolbar-actions">
            <SearchBox
              className="filter-field"
              placeholder="按名称或版本筛选"
              aria-label="筛选实例"
              value={query}
              onChange={(_, data) => setQuery(data.value)}
            />
            <Button
              appearance="subtle"
              icon={<ArrowSync20Regular />}
              aria-label="刷新实例列表"
              disabled={status === "loading"}
              onClick={reload}
            />
          </div>
        </div>
      ) : null}

      {status === "error" && error ? (
        <ErrorState title="读取实例失败" message={error} onRetry={reload} retryLabel="重新读取" />
      ) : status === "loading" && instances.length === 0 ? (
        <div className="instance-grid" aria-hidden="true">
          {[0, 1, 2].map((index) => (
            <div className="instance-card skeleton-card" key={index} />
          ))}
        </div>
      ) : instances.length === 0 ? (
        <EmptyState
          icon={<Box24Regular />}
          title="还没有实例"
          body={
            desktop
              ? "到「下载」页装一个新版本，或者把别的启动器里已有的版本导入进来。"
              : "浏览器预览里无法读取本机实例，打开桌面版即可看到列表。"
          }
          action={
            desktop ? (
              <div className="page-actions">
                <Button
                  appearance="secondary"
                  icon={<ArrowImport20Regular />}
                  onClick={() => setImporting(true)}
                >
                  导入实例
                </Button>
                <Button
                  appearance="primary"
                  icon={<ArrowDownload20Regular />}
                  onClick={() => navigate("download")}
                >
                  安装新版本
                </Button>
              </div>
            ) : undefined
          }
        />
      ) : visible.length === 0 ? (
        <EmptyState
          icon={<Warning24Regular />}
          title="没有匹配的实例"
          body={`没有名称或版本包含「${query.trim()}」的实例，换个关键词试试。`}
        />
      ) : (
        <ul className="instance-grid">
          {visible.map((instance, index) => (
            <li
              className="instance-card"
              key={instance.id}
              style={{ "--i": index } as CSSProperties}
            >
              <InstanceCover version={instance.version} />
              <div className="instance-card-body">
                <Text weight="semibold" size={400} className="instance-card-name" truncate wrap={false}>
                  {instance.name}
                </Text>
                <Text className="muted instance-card-meta" size={200}>
                  Vanilla {instance.version} · 内存 {memoryLabel}
                </Text>
                <GameControls instance={instance} />
              </div>
            </li>
          ))}
        </ul>
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

      <ImportInstancesDialog
        open={importing}
        onOpenChange={setImporting}
        onImported={reload}
      />
    </>
  );
}
