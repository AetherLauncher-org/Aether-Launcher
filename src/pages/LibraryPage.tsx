import {
  Badge,
  Button,
  SearchBox,
  Skeleton,
  SkeletonItem,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  ArrowSync24Regular,
  Filter24Regular,
  Library24Regular,
} from "@fluentui/react-icons";
import { useMemo, useState } from "react";
import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import {
  aggregateSources,
  describeResourceKind,
  filterResources,
  sortResources,
  summariseResources,
} from "../lib/library";
import type { ResourceEntry, ResourceSortKey } from "../lib/library";
import type { PageDefinition } from "../navigation";

type LibraryPageProps = {
  page: PageDefinition;
};

/** 模块级空列表：资源来源校验接上之前，列表恒为空。 */
const emptyResources: readonly ResourceEntry[] = [];

/**
 * 资源库页。
 *
 * 结构和交互是完整的（搜索、来源过滤、排序、表格），但**没有数据源**：
 * 下载计划与来源校验还没实现，所以列表恒为空，页面用空状态说明原因。
 * 不要在这里塞假数据——那样会让用户以为资源真的可以下载。
 */
export function LibraryPage({ page }: LibraryPageProps) {
  const [query, setQuery] = useState("");
  const [source, setSource] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<ResourceSortKey>("name");

  // 目前恒为空；接上后端后换成查询结果即可。
  // 用模块级常量，避免每次渲染生成新数组把 useMemo 的依赖打穿。
  const entries = emptyResources;
  const loading = false;

  const sources = useMemo(() => aggregateSources(entries), [entries]);
  const visible = useMemo(
    () => sortResources(filterResources(entries, { query, source }), sortKey),
    [entries, query, source, sortKey],
  );

  const hasFilters = query.trim().length > 0 || source !== null;

  return (
    <>
      <PageHeader
        title={page.label}
        description={page.summary}
        action={
          <Tooltip
            content="刷新需要下载源，资源来源校验完成后可用"
            relationship="description"
          >
            <Button icon={<ArrowSync24Regular />} disabled aria-label="刷新资源">
              刷新
            </Button>
          </Tooltip>
        }
      />

      <section
        className="status-panel list-panel"
        aria-labelledby="library-heading"
      >
        <div className="list-toolbar">
          <div className="toolbar-heading">
            <Text id="library-heading" as="h2" size={400} weight="semibold">
              资源条目
            </Text>
            <Badge appearance="outline">{summariseResources(entries)}</Badge>
          </div>

          <div className="toolbar-actions">
            <SearchBox
              className="filter-field"
              placeholder="按名称、版本或来源搜索"
              aria-label="搜索资源"
              value={query}
              onChange={(_, data) => setQuery(data.value)}
            />
            <label className="source-filter">
              <Filter24Regular aria-hidden="true" />
              <span className="visually-hidden">来源</span>
              <select
                className="native-select"
                aria-label="来源过滤器"
                value={source ?? ""}
                onChange={(event) => {
                  const value = event.target.value;
                  setSource(value === "" ? null : value);
                }}
              >
                <option value="">全部来源</option>
                {sources.map((facet) => (
                  <option value={facet.source} key={facet.source}>
                    {facet.source}（{facet.count}）
                  </option>
                ))}
              </select>
            </label>
            <label className="source-filter">
              <span className="visually-hidden">排序</span>
              <select
                className="native-select"
                aria-label="排序方式"
                value={sortKey}
                onChange={(event) => {
                  setSortKey(event.target.value as ResourceSortKey);
                }}
              >
                <option value="name">按名称</option>
                <option value="size">按大小</option>
                <option value="gameVersion">按版本</option>
              </select>
            </label>
          </div>
        </div>

        {loading ? (
          <ResourceTableSkeleton />
        ) : visible.length === 0 ? (
          <EmptyState
            icon={<Library24Regular />}
            title={
              hasFilters ? "没有匹配的资源" : "资源来源校验完成后可用"
            }
            body={
              hasFilters
                ? "当前没有数据，过滤条件不会命中任何条目。清空搜索和来源过滤器试试。"
                : "下载计划与来源校验还在开发。完成后这里会列出模组、资源包、光影与数据包，并显示每个文件的来源和校验值。"
            }
            action={
              hasFilters ? (
                <Button
                  appearance="secondary"
                  onClick={() => {
                    setQuery("");
                    setSource(null);
                  }}
                >
                  清空过滤器
                </Button>
              ) : undefined
            }
          />
        ) : (
          <ResourceTable entries={visible} />
        )}
      </section>

      <Text className="page-footnote muted">
        列表只展示已通过来源校验的条目，每个文件都会记录来源与 SHA-1。
      </Text>
    </>
  );
}

/** 表格骨架：列宽与真实表格一致，加载时不会跳版。 */
function ResourceTableSkeleton({ rows = 4 }: { rows?: number }) {
  return (
    <div className="resource-table-skeleton" aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <div className="resource-skeleton-row" key={index}>
          <Skeleton>
            <SkeletonItem size={14} />
          </Skeleton>
          <Skeleton>
            <SkeletonItem size={14} />
          </Skeleton>
          <Skeleton>
            <SkeletonItem size={14} />
          </Skeleton>
          <Skeleton>
            <SkeletonItem shape="rectangle" size={16} />
          </Skeleton>
        </div>
      ))}
    </div>
  );
}

function ResourceTable({ entries }: { entries: readonly ResourceEntry[] }) {
  return (
    <div className="resource-table" role="table" aria-label="资源列表">
      <div className="resource-row resource-head" role="row">
        <span role="columnheader">名称</span>
        <span role="columnheader">类型</span>
        <span role="columnheader">版本</span>
        <span role="columnheader">来源</span>
      </div>
      {entries.map((entry) => (
        <div className="resource-row" role="row" key={entry.id}>
          <span role="cell" className="resource-name">
            {entry.name}
          </span>
          <span role="cell">
            <Badge appearance="tint">{describeResourceKind(entry.kind)}</Badge>
          </span>
          <span role="cell" className="muted">
            {entry.gameVersion}
          </span>
          <span role="cell" className="muted resource-source">
            {entry.source} · {entry.host}
          </span>
        </div>
      ))}
    </div>
  );
}
