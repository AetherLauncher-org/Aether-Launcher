import { Badge, Button, SearchBox, Tab, TabList, Text } from "@fluentui/react-components";
import { ArrowSync20Regular, ChevronRight16Regular, Cube24Regular, Search24Regular } from "@fluentui/react-icons";
import { useMemo, useState } from "react";
import type { CSSProperties } from "react";
import type { MinecraftCatalogVersion, MinecraftVersionCatalog } from "../ipc";
import {
  availability,
  describeKind,
  filterVersions,
  formatReleaseDate,
  kindOf,
  versionKinds,
} from "../lib/versions";
import type { VersionKind } from "../lib/versions";
import { EmptyState } from "./EmptyState";

type VersionBrowserProps = {
  catalog: MinecraftVersionCatalog;
  /** 已经有实例的版本号，用来显示「已安装」 */
  installed: ReadonlySet<string>;
  onSelect: (version: MinecraftCatalogVersion) => void;
  onRefresh: () => void;
  refreshing: boolean;
};

/**
 * 版本总览：最新的正式版和快照放在顶部，下面是按类别分组的完整列表，可以搜索。
 * 点一行进入该版本的详情，在那里安装——列表本身不带任何下载操作。
 */
export function VersionBrowser({ catalog, installed, onSelect, onRefresh, refreshing }: VersionBrowserProps) {
  const [kind, setKind] = useState<VersionKind>("release");
  const [query, setQuery] = useState("");

  const byId = useMemo(() => new Map(catalog.versions.map((version) => [version.id, version])), [catalog]);
  const latestRelease = byId.get(catalog.latest.release);
  const latestSnapshot = byId.get(catalog.latest.snapshot);

  const visible = useMemo(() => filterVersions(catalog.versions, { kind, query }), [catalog, kind, query]);
  const latestIds = useMemo(
    () => new Set([catalog.latest.release, catalog.latest.snapshot]),
    [catalog],
  );

  const featured = [
    { label: "最新正式版", version: latestRelease },
    { label: "最新快照", version: latestSnapshot },
  ].filter((entry): entry is { label: string; version: MinecraftCatalogVersion } => entry.version !== undefined);

  return (
    <>
      {featured.length > 0 ? (
        <div className="featured-row">
          {featured.map((entry, index) => (
            <button
              type="button"
              key={entry.label}
              className="featured-card"
              style={{ "--i": index } as CSSProperties}
              onClick={() => onSelect(entry.version)}
            >
              <span className="featured-icon" aria-hidden="true">
                <Cube24Regular />
              </span>
              <span className="featured-copy">
                <Text size={200} className="muted">
                  {entry.label}
                </Text>
                <Text size={500} weight="semibold">
                  {entry.version.id}
                </Text>
                <Text size={200} className="muted">
                  {formatReleaseDate(entry.version.releaseTime)}
                </Text>
              </span>
              <ChevronRight16Regular aria-hidden="true" className="row-chevron" />
            </button>
          ))}
        </div>
      ) : null}

      <div className="toolbar">
        <TabList
          selectedValue={kind}
          onTabSelect={(_, data) => setKind(data.value as VersionKind)}
          aria-label="版本类别"
        >
          {versionKinds.map((value) => (
            <Tab key={value} value={value}>
              {describeKind(value)}
            </Tab>
          ))}
        </TabList>
        <div className="toolbar-actions">
          <SearchBox
            className="filter-field"
            placeholder="搜索版本号，例如 1.21"
            aria-label="搜索版本"
            value={query}
            onChange={(_, data) => setQuery(data.value)}
          />
          <Button
            appearance="subtle"
            icon={<ArrowSync20Regular />}
            aria-label="刷新版本目录"
            disabled={refreshing}
            onClick={onRefresh}
          />
        </div>
      </div>

      {visible.length === 0 ? (
        <EmptyState
          icon={<Search24Regular />}
          title="没有匹配的版本"
          body={query.trim() ? `没有版本号包含「${query.trim()}」，换个关键词或类别试试。` : "这个类别下没有版本。"}
        />
      ) : (
        <ul className="version-list" aria-label={`${describeKind(kind)}列表，共 ${visible.length} 个`}>
          {visible.map((version) => {
            const support = availability(version);
            return (
              <li key={version.id}>
                <button type="button" className="version-row" onClick={() => onSelect(version)}>
                  <span className={`version-tile kind-${kindOf(version)}`} aria-hidden="true">
                    <Cube24Regular />
                  </span>
                  <span className="version-main">
                    <Text weight="semibold">{version.id}</Text>
                    <Text size={200} className="muted">
                      {describeKind(kindOf(version))} · {formatReleaseDate(version.releaseTime)}
                    </Text>
                  </span>
                  <span className="version-badges">
                    {latestIds.has(version.id) ? (
                      <Badge appearance="tint" color="brand">
                        最新
                      </Badge>
                    ) : null}
                    {installed.has(version.id) ? (
                      <Badge appearance="tint" color="success">
                        已安装
                      </Badge>
                    ) : null}
                    {!support.ok ? (
                      <Badge appearance="outline" color="subtle">
                        暂不支持
                      </Badge>
                    ) : null}
                  </span>
                  <ChevronRight16Regular aria-hidden="true" className="row-chevron" />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
