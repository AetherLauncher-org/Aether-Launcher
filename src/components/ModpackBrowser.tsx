import {
  Badge, Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent,
  DialogSurface, DialogTitle, Field, Input, MessageBar, MessageBarBody,
  SearchBox, Select, Spinner, Text,
} from "@fluentui/react-components";
import { ArrowDownload20Regular, Box24Regular, Open16Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import { describeCommandError, openContentProject, searchContent } from "../ipc";
import type { SearchHit, SearchSort } from "../ipc";
import { categoryLabel, describeVersionRange, formatDownloads, loaderLabels, searchSorts, sortLabels } from "../lib/contentBrowse";
import { validateInstanceName } from "../lib/instances";
import { useQuery } from "../lib/query";
import { suggestInstanceName } from "../lib/versions";
import { useInstall } from "../state/install";
import { useInstances } from "../state/instances";
import { EmptyState } from "./EmptyState";
import { InstallFailure, InstallProgressCard } from "./InstallProgressCard";
import { ErrorState } from "./PageState";

const PAGE = 20;
type Selection = { projectId: string | null; title: string };

export function ModpackBrowser() {
  const [draft, setDraft] = useState("");
  const [text, setText] = useState("");
  const [sort, setSort] = useState<SearchSort>("downloads");
  const [offset, setOffset] = useState(0);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const { job, start, cancel, dismiss } = useInstall();
  const load = useCallback(() => searchContent(text, "modpack", null, null, [], sort, offset), [text, sort, offset]);
  const results = useQuery(load);
  const running = job?.status === "running";
  const packJob = job?.modpack ? job : null;
  const submit = () => { setText(draft); setOffset(0); };
  const openPage = async (hit: SearchHit) => {
    try { await openContentProject(hit.slug); }
    catch (error: unknown) { setFailure(describeCommandError(error)); }
  };

  return (
    <section className="content-browser" aria-label="整合包">
      <div className="toolbar">
        <div>
          <Text as="h2" size={500} weight="semibold">一个整合包，一个新实例</Text>
          <Text as="p" className="muted">从 Modrinth 挑选，或安装本地 .mrpack 文件。</Text>
        </div>
        <Button disabled={running} onClick={() => setSelection({ projectId: null, title: "本地整合包" })}>
          安装本地 .mrpack
        </Button>
      </div>
      <div className="toolbar">
        <SearchBox className="filter-field" aria-label="搜索整合包" placeholder="搜索整合包"
          value={draft} onChange={(_, data) => setDraft(data.value)}
          onKeyDown={(event) => { if (event.key === "Enter") submit(); }} />
        <div className="toolbar-actions">
          <Button appearance="primary" onClick={submit}>搜索</Button>
          <Select aria-label="排序" value={sort} onChange={(_, data) => { setSort(data.value as SearchSort); setOffset(0); }}>
            {searchSorts.map((value) => <option key={value} value={value}>{sortLabels[value]}</option>)}
          </Select>
        </div>
      </div>
      {failure ? <MessageBar intent="error" role="alert"><MessageBarBody>{failure}</MessageBarBody></MessageBar> : null}
      {running && job ? <InstallProgressCard job={job} onCancel={cancel} /> : null}
      {packJob && (packJob.status === "error" || packJob.status === "cancelled") ? (
        <InstallFailure job={packJob} onRetry={() => void start(packJob.version, packJob.name, null, packJob.modpack)} />
      ) : null}
      {packJob?.status === "done" ? (
        <MessageBar intent="success" role="status"><MessageBarBody>
          已安装「{packJob.instance?.name}」，可在实例页查看。
          <Button appearance="transparent" size="small" onClick={dismiss}>关闭</Button>
        </MessageBarBody></MessageBar>
      ) : null}
      {results.status === "error" && results.error ? (
        <ErrorState title="搜索失败" message={results.error} onRetry={results.reload} retryLabel="重试" />
      ) : results.data === null ? <div className="loading-block" role="status"><Spinner label="正在搜索整合包…" /></div>
        : results.data.hits.length === 0 ? <EmptyState title="没有找到整合包" body="换个关键词试试，或安装本地 .mrpack 文件。" /> : (
          <>
            <ul className="search-results">
              {results.data.hits.map((hit) => (
                <li className="search-hit" key={hit.projectId}>
                  <div className="search-hit-icon" aria-hidden="true">{hit.icon ? <img src={hit.icon} alt="" draggable={false} /> : <Box24Regular />}</div>
                  <div className="search-hit-main">
                    <div className="search-hit-title"><Text weight="semibold" size={400}>{hit.title}</Text>
                      {hit.author ? <Text size={200} className="muted">作者 {hit.author}</Text> : null}</div>
                    <Text size={200} className="search-hit-summary">{hit.summary}</Text>
                    <div className="search-hit-tags">
                      {hit.loaders.map((loader) => <Badge key={loader} appearance="tint" color="brand">{loaderLabels[loader] ?? loader}</Badge>)}
                      {hit.categories.map((tag) => <Badge key={tag} appearance="tint" color="informative">{categoryLabel(tag)}</Badge>)}
                    </div>
                  </div>
                  <div className="search-hit-side">
                    <Text size={200} className="muted">{formatDownloads(hit.downloads)} 次下载</Text>
                    <Text size={200} className="muted">{describeVersionRange(hit.oldestVersion, hit.newestVersion)}</Text>
                    <div className="search-hit-actions">
                      <Button size="small" appearance="subtle" icon={<Open16Regular />} aria-label={`在 Modrinth 打开 ${hit.title}`} onClick={() => void openPage(hit)} />
                      <Button size="small" appearance="primary" icon={<ArrowDownload20Regular />} aria-label={`安装 ${hit.title}`} disabled={running} onClick={() => setSelection(hit)}>安装</Button>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
            <div className="toolbar"><Text className="muted">第 {Math.floor(offset / PAGE) + 1} 页 · 共 {results.data.total.toLocaleString()} 个</Text>
              <div className="toolbar-actions">
                <Button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>上一页</Button>
                <Button disabled={offset + PAGE >= results.data.total} onClick={() => setOffset(offset + PAGE)}>下一页</Button>
              </div>
            </div>
          </>
        )}
      {selection ? <ModpackInstallDialog selection={selection} onClose={() => setSelection(null)} /> : null}
    </section>
  );
}

function ModpackInstallDialog({ selection, onClose }: { selection: Selection; onClose: () => void }) {
  const { instances } = useInstances();
  const { start, job } = useInstall();
  const [name, setName] = useState(() => suggestInstanceName(selection.title, instances.map((instance) => instance.name)));
  const [optional, setOptional] = useState(false);
  const duplicate = instances.some((instance) => instance.name.toLowerCase() === name.trim().toLowerCase());
  const error = validateInstanceName(name) ?? (duplicate ? "已经有同名实例了" : null);
  return (
    <Dialog open onOpenChange={(_, data) => { if (!data.open) onClose(); }}>
      <DialogSurface><DialogBody>
        <DialogTitle>安装{selection.projectId === null ? "本地整合包" : `「${selection.title}」`}</DialogTitle>
        <DialogContent className="dialog-content">
          <Text>{selection.projectId === null ? "点击安装后，选择一个 .mrpack 文件。" : "安装项目的最新整合包版本，创建独立实例。"}</Text>
          <Field label="实例名称" validationState={error ? "error" : "none"} validationMessage={error ?? undefined}>
            <Input value={name} onChange={(_, data) => setName(data.value)} />
          </Field>
          <Checkbox label="安装可选客户端文件" checked={optional} onChange={(_, data) => setOptional(data.checked === true)} />
          <Text size={200} className="muted">默认只安装必需文件。勾选后，还会安装整合包标记为客户端可选的文件。</Text>
        </DialogContent>
        <DialogActions>
          <Button onClick={onClose}>取消</Button>
          <Button appearance="primary" disabled={error !== null || job?.status === "running"} onClick={() => {
            void start(selection.title, name.trim(), null, { projectId: selection.projectId, optional });
            onClose();
          }}>安装</Button>
        </DialogActions>
      </DialogBody></DialogSurface>
    </Dialog>
  );
}
