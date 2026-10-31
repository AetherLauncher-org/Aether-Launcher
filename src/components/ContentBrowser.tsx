import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  SearchBox,
  Select,
  Spinner,
  Switch,
  Tab,
  TabList,
  Text,
} from "@fluentui/react-components";
import { ArrowDownload20Regular, Open16Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import {
  contentTypes,
  describeCommandError,
  getContentTarget,
  installContent,
  listInstanceContent,
  openContentProject,
  searchContent,
} from "../ipc";
import type { ContentType, InstallReport, SearchHit, SearchSort } from "../ipc";
import {
  cannotInstallReason,
  contentTypeLabels,
  describeTarget,
  formatDownloads,
  searchSorts,
  sortLabels,
} from "../lib/contentBrowse";
import { useQuery } from "../lib/query";
import { useInstances } from "../state/instances";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./PageState";

const PAGE = 20;

/**
 * 在 Modrinth 上找模组、数据包、资源包和光影，装进实例（数据包装进选定的世界，可以多选）。
 *
 * 搜索默认按所选实例的游戏版本和加载器筛。装之前不选文件：后端取最新的合适版本，按 Modrinth
 * 给的哈希校验，已有同名文件不覆盖。这里只传实例 id、项目 id、内容类型和世界文件夹名。
 */
export function ContentBrowser() {
  const { instances } = useInstances();
  const [instanceId, setInstanceId] = useState<string | null>(null);
  const [type, setType] = useState<ContentType>("mod");
  const [draft, setDraft] = useState("");
  const [text, setText] = useState("");
  const [sort, setSort] = useState<SearchSort>("downloads");
  const [filterByInstance, setFilterByInstance] = useState(true);
  const [withDependencies, setWithDependencies] = useState(true);
  const [offset, setOffset] = useState(0);

  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [report, setReport] = useState<{ title: string; result: InstallReport } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [picking, setPicking] = useState<SearchHit | null>(null);

  // 没选过就用第一个实例；选的那个被删了也回到第一个
  const current = instances.find((instance) => instance.id === instanceId) ?? instances[0] ?? null;
  const currentId = current?.id ?? null;

  const loadTarget = useCallback(
    () => (currentId === null ? Promise.reject(new Error("没有实例")) : getContentTarget(currentId)),
    [currentId],
  );
  const target = useQuery(loadTarget, currentId !== null);

  const gameVersion = filterByInstance ? (target.data?.gameVersion ?? null) : null;
  const loader = filterByInstance && type === "mod" ? (target.data?.loader ?? null) : null;
  const loadResults = useCallback(
    () => searchContent(text, type, gameVersion, loader, sort, offset),
    [text, type, gameVersion, loader, sort, offset],
  );
  const results = useQuery(loadResults, currentId !== null && (target.data !== null || !filterByInstance));

  // 换了搜索条件就回到第一页
  const refine = <T,>(set: (value: T) => void) => (value: T) => {
    set(value);
    setOffset(0);
  };
  const chooseInstance = refine(setInstanceId);
  const chooseType = refine(setType);
  const chooseSort = refine(setSort);
  const chooseFilter = refine(setFilterByInstance);
  const submit = refine(setText);

  const blocked = cannotInstallReason(type, target.data);

  const install = async (hit: SearchHit, world: string | null) => {
    if (currentId === null) {
      return;
    }
    setFailure(null);
    setReport(null);
    setBusy((previous) => new Set(previous).add(hit.projectId));
    try {
      const result = await installContent(
        currentId,
        hit.projectId,
        type,
        world,
        withDependencies && type === "mod",
      );
      setReport({ title: hit.title, result });
    } catch (error: unknown) {
      setFailure(`${hit.title}：${describeCommandError(error)}`);
    } finally {
      setBusy((previous) => {
        const next = new Set(previous);
        next.delete(hit.projectId);
        return next;
      });
    }
  };

  const openPage = async (hit: SearchHit) => {
    try {
      await openContentProject(hit.slug);
    } catch (error: unknown) {
      setFailure(describeCommandError(error));
    }
  };

  if (current === null) {
    return <EmptyState title="先有一个实例" body="内容是装进实例里的。先在「游戏版本」里安装一个版本。" />;
  }

  const hits = results.data?.hits ?? [];
  const total = results.data?.total ?? 0;

  return (
    <section className="content-browser" aria-label="内容">
      <div className="toolbar">
        <div className="toolbar-actions">
          <Select
            aria-label="装进哪个实例"
            value={current.id}
            onChange={(_, data) => chooseInstance(data.value)}
          >
            {instances.map((instance) => (
              <option key={instance.id} value={instance.id}>
                {instance.name}
              </option>
            ))}
          </Select>
          {target.data ? <Badge appearance="tint">{describeTarget(target.data)}</Badge> : null}
          <Switch
            label="只看适合这个实例的"
            checked={filterByInstance}
            onChange={(_, data) => chooseFilter(data.checked)}
          />
        </div>
        <div className="toolbar-actions">
          <Select aria-label="排序" value={sort} onChange={(_, data) => chooseSort(data.value as SearchSort)}>
            {searchSorts.map((value) => (
              <option key={value} value={value}>
                {sortLabels[value]}
              </option>
            ))}
          </Select>
        </div>
      </div>

      <TabList
        selectedValue={type}
        onTabSelect={(_, data) => chooseType(data.value as ContentType)}
        aria-label="内容类型"
      >
        {contentTypes.map((value) => (
          <Tab key={value} value={value}>
            {contentTypeLabels[value]}
          </Tab>
        ))}
      </TabList>

      <div className="toolbar">
        <SearchBox
          className="filter-field"
          placeholder={`搜索${contentTypeLabels[type]}`}
          aria-label="搜索内容"
          value={draft}
          onChange={(_, data) => setDraft(data.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              submit(draft);
            }
          }}
        />
        <div className="toolbar-actions">
          <Button appearance="primary" onClick={() => submit(draft)}>
            搜索
          </Button>
          {type === "mod" ? (
            <Switch
              label="同时装必需的前置"
              checked={withDependencies}
              onChange={(_, data) => setWithDependencies(data.checked)}
            />
          ) : null}
        </div>
      </div>

      {blocked ? (
        <MessageBar intent="warning" className="page-notice">
          <MessageBarBody>{blocked}</MessageBarBody>
        </MessageBar>
      ) : null}
      {failure ? (
        <MessageBar intent="error" className="page-notice" role="alert">
          <MessageBarBody>{failure}</MessageBarBody>
        </MessageBar>
      ) : null}
      {report ? (
        <MessageBar intent={report.result.skipped.length > 0 ? "warning" : "success"} className="page-notice" role="status">
          <MessageBarBody>
            已装 {report.title}：{report.result.installed.map((file) => file.fileName).join("、")}
            {report.result.skipped.length > 0
              ? `；${report.result.skipped.length} 个前置没能装上（${report.result.skipped.map((item) => item.reason).join("；")}）`
              : ""}
          </MessageBarBody>
        </MessageBar>
      ) : null}

      {results.status === "error" && results.error ? (
        <ErrorState title="搜索失败" message={results.error} onRetry={results.reload} retryLabel="重试" />
      ) : results.data === null ? (
        <div className="loading-block" role="status">
          <Spinner label="正在搜索…" />
        </div>
      ) : hits.length === 0 ? (
        <EmptyState title="没有找到" body="换个关键词，或者关掉「只看适合这个实例的」试试。" />
      ) : (
        <>
          <ul className="search-results">
            {hits.map((hit) => (
              <li className="search-hit" key={hit.projectId}>
                <div className="search-hit-main">
                  <Text weight="semibold" truncate wrap={false}>
                    {hit.title}
                  </Text>
                  <Text size={200} className="muted">
                    {hit.author ? `${hit.author} · ` : ""}
                    {formatDownloads(hit.downloads)} 次下载
                  </Text>
                  <Text size={200} className="muted content-card-summary">
                    {hit.summary}
                  </Text>
                </div>
                <div className="search-hit-actions">
                  <Button
                    size="small"
                    appearance="subtle"
                    icon={<Open16Regular />}
                    aria-label={`在 Modrinth 打开 ${hit.title}`}
                    onClick={() => void openPage(hit)}
                  />
                  <Button
                    size="small"
                    appearance="primary"
                    icon={busy.has(hit.projectId) ? <Spinner size="tiny" /> : <ArrowDownload20Regular />}
                    aria-label={`安装 ${hit.title}`}
                    disabled={blocked !== null || busy.has(hit.projectId)}
                    onClick={() => (type === "datapack" ? setPicking(hit) : void install(hit, null))}
                  >
                    安装
                  </Button>
                </div>
              </li>
            ))}
          </ul>
          <div className="toolbar">
            <Text className="muted">
              第 {Math.floor(offset / PAGE) + 1} 页 · 共 {total.toLocaleString()} 个
            </Text>
            <div className="toolbar-actions">
              <Button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
                上一页
              </Button>
              <Button disabled={offset + PAGE >= total} onClick={() => setOffset(offset + PAGE)}>
                下一页
              </Button>
            </div>
          </div>
        </>
      )}

      <WorldPicker
        instanceId={currentId}
        hit={picking}
        onClose={() => setPicking(null)}
        onPick={async (worlds) => {
          const hit = picking;
          setPicking(null);
          if (hit !== null) {
            for (const world of worlds) {
              await install(hit, world);
            }
          }
        }}
      />
    </section>
  );
}

type WorldPickerProps = {
  instanceId: string | null;
  hit: SearchHit | null;
  onClose: () => void;
  onPick: (worlds: string[]) => void | Promise<void>;
};

/** 数据包装进哪些世界：列出实例的世界，可以多选。 */
function WorldPicker({ instanceId, hit, onClose, onPick }: WorldPickerProps) {
  const open = hit !== null && instanceId !== null;
  const load = useCallback(() => listInstanceContent(instanceId ?? "", "saves"), [instanceId]);
  const { data, status, error } = useQuery(load, open);
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());

  return (
    <Dialog
      open={open}
      onOpenChange={(_, next) => {
        if (!next.open) {
          setChosen(new Set());
          onClose();
        }
      }}
    >
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{hit ? `把「${hit.title}」装进哪些世界？` : ""}</DialogTitle>
          <DialogContent className="dialog-content">
            {status === "error" && error ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            ) : data === null ? (
              <Spinner label="正在读取世界…" />
            ) : data.length === 0 ? (
              <Text>这个实例还没有世界。先进游戏创建一个，数据包要装进某个世界。</Text>
            ) : (
              data.map((world) => (
                <Checkbox
                  key={world.name}
                  label={world.name}
                  checked={chosen.has(world.name)}
                  onChange={(_, state) =>
                    setChosen((previous) => {
                      const next = new Set(previous);
                      if (state.checked === true) {
                        next.add(world.name);
                      } else {
                        next.delete(world.name);
                      }
                      return next;
                    })
                  }
                />
              ))
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onClose}>
              取消
            </Button>
            <Button
              appearance="primary"
              disabled={chosen.size === 0}
              onClick={() => {
                const worlds = [...chosen];
                setChosen(new Set());
                void onPick(worlds);
              }}
            >
              安装
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
