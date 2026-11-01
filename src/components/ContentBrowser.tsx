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
  ProgressBar,
  SearchBox,
  Select,
  Spinner,
  Switch,
  Tab,
  TabList,
  Text,
} from "@fluentui/react-components";
import {
  ArrowDownload16Regular,
  ArrowDownload20Regular,
  Box24Regular,
  Heart16Regular,
  Open16Regular,
} from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import {
  describeCommandError,
  getContentTarget,
  listInstanceContent,
  openContentProject,
  searchContent,
} from "../ipc";
import type { ContentType, SearchHit, SearchSort } from "../ipc";
import { loadCatalog } from "../lib/catalogCache";
import {
  cannotInstallReason,
  categoryLabel,
  categoryOptions,
  contentTypeLabels,
  describeSide,
  describeTarget,
  describeVersionRange,
  filterLoaders,
  formatDownloads,
  formatUpdated,
  loaderLabels,
  searchSorts,
  sortLabels,
} from "../lib/contentBrowse";
import { formatBytes } from "../lib/install";
import { useQuery } from "../lib/query";
import { useInstances } from "../state/instances";
import { useContentInstalls } from "../state/contentInstall";
import type { ContentInstallTask } from "../state/contentInstall";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./PageState";

const PAGE = 20;

/**
 * 在 Modrinth 上找模组、数据包、资源包和光影，装进实例（数据包装进选定的世界，可以多选）。
 *
 * 两种筛法，都可以再叠加分类标签：
 * - 按实例：选一个实例，按它的游戏版本和加载器筛，可以直接安装进去。
 * - 自选条件：自己选加载器、游戏版本；只用来浏览，不装进任何实例。
 *
 * 装之前不选文件：后端取最新的合适版本，按 Modrinth 给的哈希校验，已有同名文件不覆盖。
 * 这里只传实例 id、项目 id、内容类型和世界文件夹名。
 */
type ContentBrowserProps = {
  /** 搜索哪一类：由下载页顶上的标签决定 */
  type: ContentType;
};

type Mode = "instance" | "manual";

/** 选过的标签里，这一类内容有的那些（换了内容类型，上一类的标签不再适用）。 */
function tagsFor(type: ContentType, picked: readonly string[]): string[] {
  return picked.filter((tag) => categoryOptions[type].includes(tag));
}

export function ContentBrowser({ type }: ContentBrowserProps) {
  const { instances } = useInstances();
  const [instanceId, setInstanceId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [text, setText] = useState("");
  const [sort, setSort] = useState<SearchSort>("downloads");
  const [mode, setMode] = useState<Mode>("instance");
  const [manualVersion, setManualVersion] = useState<string | null>(null);
  const [manualLoader, setManualLoader] = useState<string | null>(null);
  const [pickedTags, setPickedTags] = useState<readonly string[]>([]);
  const [withDependencies, setWithDependencies] = useState(true);
  const [offset, setOffset] = useState(0);

  const { tasks, enqueue, cancel, dismiss } = useContentInstalls();
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

  // 自选条件下的游戏版本：官方版本目录里的正式版（只在自选条件下读）
  const loadReleases = useCallback(
    async () => (await loadCatalog()).versions.filter((version) => version.type === "release").map((version) => version.id),
    [],
  );
  const releases = useQuery(loadReleases, mode === "manual");

  const byInstance = mode === "instance";
  const gameVersion = byInstance ? (target.data?.gameVersion ?? null) : manualVersion;
  const loader = type !== "mod" ? null : byInstance ? (target.data?.loader ?? null) : manualLoader;
  // 切换内容类型后，上一类的标签不再适用
  const tagChoices = categoryOptions[type];
  const tags = tagsFor(type, pickedTags);
  const loadResults = useCallback(
    () => searchContent(text, type, gameVersion, loader, tagsFor(type, pickedTags), sort, offset),
    [text, type, gameVersion, loader, pickedTags, sort, offset],
  );
  const results = useQuery(loadResults, byInstance ? currentId !== null && target.data !== null : true);

  // 换了搜索条件就回到第一页
  const refine = <T,>(set: (value: T) => void) => (value: T) => {
    set(value);
    setOffset(0);
  };
  const chooseInstance = refine(setInstanceId);
  const chooseSort = refine(setSort);
  const chooseMode = refine(setMode);
  const chooseVersion = refine(setManualVersion);
  const chooseLoader = refine(setManualLoader);
  const chooseTags = refine(setPickedTags);
  const submit = refine(setText);
  const toggleTag = (tag: string) =>
    chooseTags(tags.includes(tag) ? tags.filter((item) => item !== tag) : [...tags, tag]);

  const blocked = byInstance
    ? cannotInstallReason(type, target.data)
    : "自选条件下只能浏览。要安装，请切到「按实例」，选一个实例。";

  const install = (hit: SearchHit, world: string | null) => {
    if (currentId === null) return;
    enqueue(hit.title, {
      kind: "install",
      id: currentId, projectId: hit.projectId, projectType: type, world,
      withDependencies: withDependencies && type === "mod",
    });
  };
  const busy = new Set(tasks.filter((task) => task.args.id === currentId &&
    (task.status === "queued" || task.status === "running")).map((task) => task.args.projectId));

  const saveFolder = (hit: SearchHit) => {
    if (currentId === null) return;
    enqueue(`保存 ${hit.title} 到文件夹`, {
      kind: "folder",
      id: currentId, projectId: hit.projectId, projectType: type,
      withDependencies: withDependencies && type === "mod",
    });
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
      <TabList
        aria-label="筛选方式"
        size="small"
        selectedValue={mode}
        onTabSelect={(_, data) => chooseMode(data.value as Mode)}
      >
        <Tab value="instance">按实例</Tab>
        <Tab value="manual">自选条件</Tab>
      </TabList>

      <div className="toolbar">
        {byInstance ? (
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
          </div>
        ) : (
          <div className="toolbar-actions">
            {type === "mod" ? (
              <Select
                aria-label="模组加载器"
                value={manualLoader ?? ""}
                onChange={(_, data) => chooseLoader(data.value === "" ? null : data.value)}
              >
                <option value="">加载器不限</option>
                {filterLoaders.map((value) => (
                  <option key={value} value={value}>
                    {loaderLabels[value] ?? value}
                  </option>
                ))}
              </Select>
            ) : null}
            <Select
              aria-label="游戏版本"
              value={manualVersion ?? ""}
              onChange={(_, data) => chooseVersion(data.value === "" ? null : data.value)}
            >
              <option value="">游戏版本不限</option>
              {(releases.data ?? []).map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </Select>
            {releases.status === "error" ? (
              <Text size={200} className="muted">
                没读到版本列表
              </Text>
            ) : null}
          </div>
        )}
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

      <div className="tag-filter" role="group" aria-label="分类标签">
        {tagChoices.map((tag) => (
          <Button
            key={tag}
            size="small"
            shape="circular"
            appearance={tags.includes(tag) ? "primary" : "outline"}
            aria-pressed={tags.includes(tag)}
            onClick={() => toggleTag(tag)}
          >
            {categoryLabel(tag)}
          </Button>
        ))}
        {tags.length > 0 ? (
          <Button size="small" appearance="subtle" onClick={() => chooseTags([])}>
            清除标签
          </Button>
        ) : null}
      </div>

      {blocked ? (
        <MessageBar intent={byInstance ? "warning" : "info"} className="page-notice">
          <MessageBarBody>{blocked}</MessageBarBody>
        </MessageBar>
      ) : null}
      {failure ? (
        <MessageBar intent="error" className="page-notice" role="alert">
          <MessageBarBody>{failure}</MessageBarBody>
        </MessageBar>
      ) : null}
      {tasks.length > 0 ? (
        <section className="content-install-tasks" aria-label="内容安装任务">
          <Text size={200} className="muted">最多同时安装 2 项，其余按顺序等待</Text>
          {tasks.map((task) => <ContentInstallCard key={task.key} task={task}
            onCancel={() => cancel(task.key)} onDismiss={() => dismiss(task.key)} />)}
        </section>
      ) : null}

      {results.status === "error" && results.error ? (
        <ErrorState title="搜索失败" message={results.error} onRetry={results.reload} retryLabel="重试" />
      ) : results.data === null ? (
        <div className="loading-block" role="status">
          <Spinner label="正在搜索…" />
        </div>
      ) : hits.length === 0 ? (
        <EmptyState title="没有找到" body="换个关键词，或者去掉一些标签、换到「自选条件」放宽版本试试。" />
      ) : (
        <>
          <ul className="search-results">
            {hits.map((hit) => {
              const range = describeVersionRange(hit.oldestVersion, hit.newestVersion);
              const sides = [describeSide("客户端", hit.clientSide), describeSide("服务端", hit.serverSide)].filter(
                (text): text is string => text !== null,
              );
              return (
                <li className="search-hit" key={hit.projectId}>
                  <div className="search-hit-icon" aria-hidden="true">
                    {hit.icon ? <img src={hit.icon} alt="" draggable={false} /> : <Box24Regular />}
                  </div>
                  <div className="search-hit-main">
                    <div className="search-hit-title">
                      <Text weight="semibold" size={400} truncate wrap={false}>
                        {hit.title}
                      </Text>
                      {hit.author ? (
                        <Text size={200} className="muted" truncate wrap={false}>
                          作者 {hit.author}
                        </Text>
                      ) : null}
                    </div>
                    <Text size={200} className="search-hit-summary">
                      {hit.summary}
                    </Text>
                    <div className="search-hit-tags">
                      {hit.loaders.map((loader) => (
                        <Badge key={loader} appearance="tint" color="brand">
                          {loaderLabels[loader] ?? loader}
                        </Badge>
                      ))}
                      {hit.categories.map((tag) => (
                        <Badge key={tag} appearance="tint" color="informative">
                          {categoryLabel(tag)}
                        </Badge>
                      ))}
                      {sides.map((text) => (
                        <Badge key={text} appearance="outline" color="subtle">
                          {text}
                        </Badge>
                      ))}
                    </div>
                  </div>
                  <div className="search-hit-side">
                    <div className="search-hit-stats">
                      <Text size={200} className="muted">
                        <ArrowDownload16Regular aria-hidden="true" /> {formatDownloads(hit.downloads)}
                      </Text>
                      <Text size={200} className="muted">
                        <Heart16Regular aria-hidden="true" /> {formatDownloads(hit.follows)}
                      </Text>
                    </div>
                    {range ? (
                      <Text size={200} className="muted">
                        {range}
                      </Text>
                    ) : null}
                    {formatUpdated(hit.updated) ? (
                      <Text size={200} className="muted">
                        更新于 {formatUpdated(hit.updated)}
                      </Text>
                    ) : null}
                    <div className="search-hit-actions">
                      <Button size="small" disabled={busy.has(hit.projectId) || blocked !== null}
                        aria-label={`保存 ${hit.title} 到文件夹`}
                        onClick={() => void saveFolder(hit)}>保存到文件夹…</Button>
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
                  </div>
                </li>
              );
            })}
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

const phaseLabels = {
  resolving: "正在解析版本与前置", downloading: "正在下载", verifying: "正在校验",
  done: "正在读取安装结果", failed: "正在读取失败结果", cancelled: "正在读取取消结果",
};

function ContentInstallCard({ task, onCancel, onDismiss }: {
  task: ContentInstallTask; onCancel: () => void; onDismiss: () => void;
}) {
  const active = task.status === "queued" || task.status === "running";
  const saving = task.args.kind === "folder";
  // 安装任务的措辞保持原样（「已装 …」「安装完成」）；保存任务换动词，句式一致。
  const verb = saving ? "保存" : "安装";
  const doneLead = saving ? `已保存 ${task.title}：` : `已装 ${task.title}：`;
  const progress = task.progress;
  const result = task.result;
  const label = task.status === "queued" ? `等待${verb}` : task.status === "running"
    ? task.cancelRequested ? "正在取消…" : progress ? phaseLabels[progress.phase] : `正在准备${verb}`
    : task.status === "done" ? (saving ? "保存完成" : "安装完成") : task.status === "cancelled" ? "已取消" : `${verb}失败`;
  const fileRatio = progress?.phase === "downloading" && progress.bytesTotal > 0
    ? Math.min(1, Math.max(0, progress.bytesDone / progress.bytesTotal)) : undefined;
  return (
    <article className="content-install-card" aria-label={`${task.title} 安装任务`}>
      <div className="content-install-heading">
        <Text weight="semibold">{task.title}{task.args.kind === "install" && task.args.world ? ` · ${task.args.world}` : ""}</Text>
        <Badge appearance="tint" color={task.status === "failed" ? "danger" : task.status === "done" ? "success" : "brand"}>{label}</Badge>
        <Button size="small" appearance="subtle" disabled={active && task.cancelRequested}
          aria-label={`${active ? "取消" : "关闭"} ${task.title}`} onClick={active ? onCancel : onDismiss}>
          {active ? "取消" : "关闭"}
        </Button>
      </div>
      {active ? <>
        <Text size={200} className="muted" role="status">
          {progress ? `${progress.filesDone} / ≥${progress.filesDiscovered} 个文件已校验 · ${label}` : label}
        </Text>
        {task.status === "running" ? <ProgressBar aria-label={`${task.title} 当前文件进度`} value={fileRatio} /> : null}
        {progress?.fileName ? <Text size={200} className="content-install-file">
          {progress.fileName}{progress.bytesTotal > 0 ? ` · ${formatBytes(progress.bytesDone)} / ${formatBytes(progress.bytesTotal)}` : ""}
        </Text> : null}
      </> : <div role={task.status === "failed" ? "alert" : "status"}>
        <Text size={200}>
          {task.status === "done" ? doneLead : `${label} ${task.title}：`}
          {result ? result.installed.length > 0
            ? `已保留并校验 ${result.installed.map((file) => file.fileName).join("、")}`
            : "未保留已校验文件" : task.operationId ? "未能读取保留文件清单" : `未启动${verb}`}
          {result?.error ? `；${result.error}` : ""}
          {result && result.skipped.length > 0
            ? `；${result.skipped.length} 个前置没能装上（${result.skipped.map((item) => `${item.projectId}：${item.reason}`).join("；")}）` : ""}
          {task.notice ? `；${task.notice}` : ""}
        </Text>
      </div>}
      {active && task.notice ? <Text size={200} role="alert">{task.notice}</Text> : null}
    </article>
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
