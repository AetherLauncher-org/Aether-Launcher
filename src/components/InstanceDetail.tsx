import { Badge, Tab, TabList, Text } from "@fluentui/react-components";
import { ArrowLeft20Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import { getInstanceDetails } from "../ipc";
import type { ContentKind, InstanceSummary } from "../ipc";
import { contentCopy } from "../lib/instanceContent";
import { useQuery } from "../lib/query";
import { useGame } from "../state/game";
import { GameControls } from "./GameControls";
import { InstanceContentTab } from "./InstanceContentTab";
import { InstanceCover } from "./InstanceCover";
import { InstanceLogsTab } from "./InstanceLogsTab";
import { InstanceOverview } from "./InstanceOverview";
import { ErrorState } from "./PageState";

type InstanceDetailProps = {
  instance: InstanceSummary;
  onBack: () => void;
  /** 实例本身变了（改名）：让列表重新读取 */
  onChanged: () => void;
  /** 实例被删掉了：回到列表 */
  onDeleted: () => void;
};

type TabId = "overview" | ContentKind | "logs";

const tabOrder: readonly TabId[] = [
  "overview",
  "mods",
  "resourcepacks",
  "shaderpacks",
  "saves",
  "schematics",
  "logs",
];

function tabLabel(tab: TabId): string {
  if (tab === "overview") {
    return "概览";
  }
  return tab === "logs" ? "日志" : contentCopy[tab].tab;
}

/**
 * 单个实例的管理页。和下载页的版本详情一样放在页面里，不另开路由：左上角返回列表，
 * 上面是封面、名字和启动按钮，下面是标签页——概览、各类内容、日志。
 */
export function InstanceDetail({ instance, onBack, onChanged, onDeleted }: InstanceDetailProps) {
  const { sessionOf } = useGame();
  const [tab, setTab] = useState<TabId>("overview");

  const load = useCallback(() => getInstanceDetails(instance.id), [instance.id]);
  const details = useQuery(load);

  const phase = sessionOf(instance.id).phase;
  const locked = phase === "starting" || phase === "running";

  const reloadDetails = details.reload;
  const handleRenamed = useCallback(() => {
    reloadDetails();
    onChanged();
  }, [reloadDetails, onChanged]);

  return (
    <div className="detail">
      <button type="button" className="back-link" onClick={onBack}>
        <ArrowLeft20Regular aria-hidden="true" />
        返回实例列表
      </button>

      <header className="detail-hero">
        <InstanceCover version={instance.version} className="detail-cover" />
        <div className="detail-hero-copy">
          <div className="detail-title-row">
            <Text as="h1" size={800} weight="semibold" className="page-title">
              {instance.name}
            </Text>
            {details.data?.loader ? (
              <Badge appearance="tint" color="brand">
                {details.data.loader}
              </Badge>
            ) : null}
          </div>
          <Text className="page-subtitle">Vanilla {instance.version}</Text>
        </div>
        <div className="hero-action">
          <GameControls instance={instance} size="large" />
        </div>
      </header>

      <TabList
        className="manage-tabs"
        selectedValue={tab}
        onTabSelect={(_, data) => setTab(data.value as TabId)}
        aria-label="实例内容"
      >
        {tabOrder.map((id) => (
          <Tab key={id} value={id}>
            {tabLabel(id)}
            {id !== "overview" && id !== "logs" && details.data !== null ? (
              <span className="tab-count">{details.data.counts[id]}</span>
            ) : null}
          </Tab>
        ))}
      </TabList>

      <div className="manage-body" role="tabpanel" aria-label={tabLabel(tab)}>
        {details.status === "error" && details.error ? (
          <ErrorState
            title="读取实例失败"
            message={details.error}
            onRetry={details.reload}
            retryLabel="重新读取"
          />
        ) : tab === "overview" ? (
          <InstanceOverview
            instance={instance}
            details={details.data}
            locked={locked}
            onRenamed={handleRenamed}
            onDeleted={onDeleted}
          />
        ) : tab === "logs" ? (
          <InstanceLogsTab instanceId={instance.id} />
        ) : (
          <InstanceContentTab
            key={tab}
            instanceId={instance.id}
            kind={tab}
            locked={locked}
            onChanged={details.reload}
          />
        )}
      </div>
    </div>
  );
}
