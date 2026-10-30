import { Text, Tooltip } from "@fluentui/react-components";
import {
  CheckmarkCircle20Filled,
  ChevronRight16Regular,
  Dismiss16Regular,
  Warning16Filled,
} from "@fluentui/react-icons";
import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { isTauri, listModLoaderVersions } from "../ipc";
import type { ModLoaderChoice, ModLoaderKind } from "../ipc";

/**
 * 「添加模组加载器」：四个加载器排成一组可展开的行。
 *
 * 版本列表来自各家的官方元数据（Fabric 的 `meta.fabricmc.net`、Forge 与
 * NeoForge 的官方 maven）。这里只负责挑选：不可用的行置灰并写明原因，可用的行
 * 点开是版本列表，选中后把结果交给调用方。
 *
 * **这里没有安装按钮。** 页面上只有一个「安装」，由 `VersionDetail` 持有：
 * 没选加载器就装原版，选了就装加载器。PCL 也是在卡片列表外放一个 `BtnStart`。
 */
export type ModLoaderSelection = {
  kind: ModLoaderKind;
  label: string;
  version: string;
};

type ModLoaderPickerProps = {
  /** 要给哪个游戏版本加加载器，例如 `1.21.1`。 */
  gameVersion: string;
  /** 当前选中的加载器；`null` 表示不装加载器（装原版）。 */
  value: ModLoaderSelection | null;
  onChange: (value: ModLoaderSelection | null) => void;
};

/** 加载器的固定顺序：PCL 的习惯是 Forge 在前，Fabric 在后。 */
const loaderOrder: readonly ModLoaderKind[] = ["forge", "neoforge", "fabric", "quilt"];

/** 还没拿到元数据时也要能画出名字的对照表。 */
const LOADER_LABELS: Record<ModLoaderKind, string> = {
  forge: "Forge",
  neoforge: "NeoForge",
  fabric: "Fabric",
  quilt: "Quilt",
};

function loaderLabel(kind: ModLoaderKind): string {
  return LOADER_LABELS[kind];
}

/** 中间那段状态文字。 */
function describeChoice(
  choice: ModLoaderChoice,
  selected: string | undefined,
  conflict: string | null,
): string {
  if (conflict !== null) {
    return conflict;
  }
  if (!choice.available) {
    return choice.reason ?? "暂不可用";
  }
  if (choice.versions.length === 0) {
    return "没有可选的版本";
  }
  return selected ?? "可以添加";
}

/** 列表里每个版本右边的标签；只标「最新」。 */
function describeVersion(choice: ModLoaderChoice, version: string): string | null {
  return choice.versions[0] === version ? "最新" : null;
}

export function ModLoaderPicker({ gameVersion, value, onChange }: ModLoaderPickerProps) {
  const desktop = isTauri();

  const [choices, setChoices] = useState<ModLoaderChoice[]>([]);
  const [loading, setLoading] = useState(desktop);
  // 递增它表示「再读一次」，effect 只依赖它，不在读取过程中同步写 state
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState<string | null>(null);

  /** 当前展开的加载器；同一时间只展开一行，和 PCL 一样。 */
  const [openKind, setOpenKind] = useState<ModLoaderKind | null>(null);

  // 读取列表是外部请求，结果回来才写 state；effect 体里只发起请求。
  useEffect(() => {
    if (!desktop) {
      return;
    }

    let cancelled = false;
    void listModLoaderVersions(gameVersion).then((found) => {
      if (cancelled) {
        return;
      }
      setChoices([...found].sort((a, b) => loaderOrder.indexOf(a.kind) - loaderOrder.indexOf(b.kind)));
      // 四个都读不到时界面上没有任何可点的行，与其空着不如给一个重新读取的入口
      setError(found.length === 0 ? "没能读取模组加载器列表" : null);
      setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [desktop, gameVersion, attempt]);

  const retry = useCallback(() => {
    setError(null);
    setLoading(true);
    setAttempt((value) => value + 1);
  }, []);

  /**
   * 某个加载器是不是被别的**已选**加载器挡着。
   *
   * 这是 PCL 的规则（`LoadForgeGetError` 里的
   * `If SelectedFabric IsNot Nothing Then Return "与 Fabric 不兼容"`）：
   * 不是一开始就把四个都标红，而是选了谁，才把与它互斥的项标出来。
   */
  const conflictWith = (choice: ModLoaderChoice): string | null => {
    if (!choice.available || value === null || value.kind === choice.kind) {
      return null;
    }
    // Forge 与 NeoForge 同族、Fabric 与 Quilt 同族：族内可以共存，由用户自己二选一。
    const sameFamily =
      ["forge", "neoforge"].includes(choice.kind) ===
      ["forge", "neoforge"].includes(value.kind);
    return sameFamily ? null : `与 ${value.label} 不兼容`;
  };

  const toggle = (choice: ModLoaderChoice) => {
    // 被挡住的项不给展开：和 PCL 的 `CardForge_PreviewSwap` 一样。
    if (!choice.available || conflictWith(choice) !== null) {
      return;
    }
    setOpenKind((current) => (current === choice.kind ? null : choice.kind));
  };

  /**
   * 选中一个版本，然后把列表收回去。
   *
   * 这是 PCL 的做法（`Forge_Selected` 里 `CardForge.IsSwapped = True`）：列表是
   * 一次性的，选完就该看到行上的结果，而不是继续占着版面。
   */
  const pick = (choice: ModLoaderChoice, version: string) => {
    setOpenKind(null);

    onChange({ kind: choice.kind, label: choice.label, version });
  };

  /** 取消一个加载器的选择，回到「装原版」。 */
  const clear = () => {
    setOpenKind(null);
    onChange(null);
  };

  const fabricChosen = value?.kind === "fabric";

  return (
    <section className="status-panel modloader-card" aria-labelledby="modloader-heading">
      <div className="modloader-head">
        <Text as="h2" id="modloader-heading" size={400} weight="semibold">
          模组加载器
        </Text>
        <Text className="muted" size={200}>
          {gameVersion}
        </Text>
      </div>

      {!desktop ? (
        <Text className="muted modloader-note" role="note">
          需要桌面版：浏览器预览拿不到模组加载器列表。
        </Text>
      ) : loading ? (
        // 列表读回来之前就先把四行画出来，和 PCL 一样：每行只写「获取中……」，
        // 数据到了再逐行替换。整块空白会让人以为卡住了。
        <ul className="modloader-list" aria-label="模组加载器" aria-busy="true">
          {loaderOrder.map((kind, index) => (
            <li
              key={kind}
              className="modloader-item"
              style={{ "--i": index } as CSSProperties}
              data-band={kind}
            >
              <div className="modloader-row" aria-disabled="true" data-available="false">
                <span className="modloader-name">
                  <span className="modloader-band" aria-hidden="true" />
                  <Text weight="semibold">{loaderLabel(kind)}</Text>
                </span>
                <span className="modloader-status">
                  <span className="modloader-status-text">获取中……</span>
                </span>
                <span className="modloader-chevron-placeholder" aria-hidden="true" />
              </div>
            </li>
          ))}
        </ul>
      ) : error !== null && choices.length === 0 ? (
        <div className="modloader-empty" role="alert">
          <Text className="modloader-error">{error}</Text>
          <button type="button" className="modloader-retry" onClick={retry}>
            重新读取
          </button>
        </div>
      ) : (
        <ul className="modloader-list" aria-label="模组加载器">
          {choices.map((choice, index) => {
            const expanded = openKind === choice.kind;
            const selected = value?.kind === choice.kind ? value.version : undefined;
            // 「不兼容」只在选了别的加载器之后才出现，和 PCL 的 SelectedXxx 一致。
            const conflict = conflictWith(choice);
            const status = describeChoice(choice, selected, conflict);
            // 被挡住的项不允许展开。
            const usable = choice.available && conflict === null;

            return (
              <li
                key={choice.kind}
                className="modloader-item"
                style={{ "--i": index } as CSSProperties}
                data-band={choice.kind}
              >
                <Tooltip
                  content={
                    conflict ??
                    (selected === undefined
                      ? `为 ${gameVersion} 添加 ${choice.label}`
                      : `${choice.label} ${selected}`)
                  }
                  relationship="description"
                  withArrow
                  positioning="above-start"
                >
                  <div className="modloader-row-wrap">
                    <button
                      type="button"
                      className="modloader-row"
                      aria-label={choice.label}
                      aria-expanded={usable ? expanded : undefined}
                      aria-controls={usable ? `modloader-versions-${choice.kind}` : undefined}
                      aria-disabled={!usable}
                      data-available={usable}
                      onClick={() => toggle(choice)}
                    >
                      <span className="modloader-name">
                        <span className="modloader-band" aria-hidden="true" />
                        <Text weight="semibold">{choice.label}</Text>
                      </span>
                      <span className="modloader-status">
                        {usable ? (
                          <CheckmarkCircle20Filled
                            className="modloader-status-icon"
                            aria-hidden="true"
                          />
                        ) : (
                          <Warning16Filled className="modloader-status-icon" aria-hidden="true" />
                        )}
                        <span className="modloader-status-text">{status}</span>
                      </span>
                      {usable ? (
                        <ChevronRight16Regular
                          aria-hidden="true"
                          className={`row-chevron${expanded ? " expanded" : ""}`}
                        />
                      ) : (
                        <span className="modloader-chevron-placeholder" aria-hidden="true" />
                      )}
                    </button>
                    {/* X：取消选择。PCL 里是 BtnForgeClear 这类按钮，只在选中后出现。 */}
                    {selected === undefined ? null : (
                      <button
                        type="button"
                        className="modloader-clear"
                        aria-label={`取消选择 ${choice.label}`}
                        onClick={clear}
                      >
                        <Dismiss16Regular aria-hidden="true" />
                      </button>
                    )}
                  </div>
                </Tooltip>

                {expanded ? (
                  <div className="modloader-body" id={`modloader-versions-${choice.kind}`}>
                    {choice.versions.length === 0 ? (
                      <Text className="muted" size={200}>
                        这个加载器还没有发布支持 {gameVersion} 的版本。
                      </Text>
                    ) : (
                      <ul
                        className="modloader-versions"
                        role="listbox"
                        aria-label={`${choice.label} 版本`}
                      >
                        {choice.versions.map((version) => {
                          const isSelected = selected === version;
                          return (
                            <li key={version}>
                              <button
                                type="button"
                                role="option"
                                aria-selected={isSelected}
                                className={`modloader-version${isSelected ? " selected" : ""}`}
                                onClick={() => pick(choice, version)}
                              >
                                <span className="modloader-version-label">{version}</span>
                                {describeVersion(choice, version) === null ? null : (
                                  <span className="modloader-version-tag">
                                    {describeVersion(choice, version)}
                                  </span>
                                )}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}

      {/*
        Fabric 模组大多依赖 Fabric API，但安装它还没有实现（要先决定 mods 放哪、
        下载校验值从哪来）。这里只如实提示，不假装会装。
      */}
      {fabricChosen ? (
        <Text className="muted modloader-note" size={200} role="note">
          Fabric 模组大多还需要 Fabric API。启动器暂时不会自动安装它，需要的话请自己下载。
        </Text>
      ) : null}
    </section>
  );
}
