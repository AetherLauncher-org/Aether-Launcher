import { Spinner, Tab, TabList } from "@fluentui/react-components";
import { useCallback, useMemo, useRef, useState } from "react";
import { ContentBrowser } from "../components/ContentBrowser";
import { PageHeader } from "../components/PageHeader";
import { DesktopOnlyNotice, ErrorState } from "../components/PageState";
import { VersionBrowser } from "../components/VersionBrowser";
import { VersionDetail } from "../components/VersionDetail";
import { contentTypes } from "../ipc";
import type { ContentType, MinecraftCatalogVersion } from "../ipc";
import { contentTypeLabels } from "../lib/contentBrowse";
import { loadCatalog } from "../lib/catalogCache";
import { useQuery } from "../lib/query";
import type { PageDefinition } from "../navigation";
import { useInstall } from "../state/install";
import { useInstances } from "../state/instances";

/** 顶上的标签：Minecraft 本体，然后是各类内容。 */
type DownloadTab = "versions" | ContentType;

type DownloadPageProps = {
  page: PageDefinition;
};

/**
 * 下载页：先看到全部版本和它们的元数据，点进某个版本才是安装。
 * 安装进行中离开再回来，会直接回到那个版本的详情，进度还在。
 */
export function DownloadPage({ page }: DownloadPageProps) {
  const { desktop, instances } = useInstances();
  const { job } = useInstall();
  // undefined：用户还没选，进行中的安装会自动打开；null：用户明确回到了列表
  const [chosen, setChosen] = useState<MinecraftCatalogVersion | null | undefined>(undefined);
  const [tab, setTab] = useState<DownloadTab>("versions");
  const force = useRef(false);

  const load = useCallback(() => {
    const refresh = force.current;
    force.current = false;
    return loadCatalog(refresh);
  }, []);
  const { status, data: catalog, error, reload } = useQuery(load, desktop);

  const refresh = useCallback(() => {
    force.current = true;
    reload();
  }, [reload]);

  const installed = useMemo(() => new Set(instances.map((instance) => instance.version)), [instances]);

  // 有安装在进行（或刚结束）时，直接展示它所属版本的详情
  const jobVersion = useMemo(
    () => (job === null || catalog === null ? null : (catalog.versions.find((item) => item.id === job.version) ?? null)),
    [job, catalog],
  );
  const selected =
    chosen !== undefined ? chosen : job !== null && job.status === "running" ? jobVersion : null;

  if (!desktop) {
    return (
      <>
        <PageHeader title={page.label} description={page.summary} />
        <DesktopOnlyNotice body="浏览器预览无法读取 Mojang 官方版本目录。用桌面版可以浏览并安装所有版本。" />
      </>
    );
  }

  if (selected !== null && catalog !== null) {
    return (
      <VersionDetail
        key={selected.id}
        version={selected}
        isLatest={selected.id === catalog.latest.release || selected.id === catalog.latest.snapshot}
        onBack={() => setChosen(null)}
      />
    );
  }

  return (
    <>
      <PageHeader title={page.label} description={page.summary} />

      <TabList selectedValue={tab} onTabSelect={(_, data) => setTab(data.value as DownloadTab)} aria-label="下载什么">
        <Tab value="versions">Minecraft 本体</Tab>
        {contentTypes.map((type) => (
          <Tab key={type} value={type}>
            {contentTypeLabels[type]}
          </Tab>
        ))}
      </TabList>

      {tab !== "versions" ? (
        <ContentBrowser type={tab} />
      ) : status === "error" && error ? (
        <ErrorState title="读取版本目录失败" message={error} onRetry={refresh} retryLabel="重新读取" />
      ) : catalog === null ? (
        <div className="loading-block" role="status">
          <Spinner label="正在读取 Mojang 官方版本目录…" />
        </div>
      ) : (
        <VersionBrowser
          catalog={catalog}
          installed={installed}
          onSelect={setChosen}
          onRefresh={refresh}
          refreshing={status === "loading"}
        />
      )}
    </>
  );
}
