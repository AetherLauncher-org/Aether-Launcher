import { Button, Text, Tooltip } from "@fluentui/react-components";
import {
  ArrowDownload20Regular,
  CheckmarkCircle20Filled,
  ChevronRight16Regular,
  Warning16Filled,
} from "@fluentui/react-icons";
import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import {
  describeCommandError,
  installModLoaderVersion,
  isTauri,
  listModLoaderVersions,
} from "../ipc";
import type { InstanceSummary, ModLoaderChoice, ModLoaderKind } from "../ipc";

/**
 * 「添加模组加载器」：四个加载器排成一组可展开的行。
 *
 * 版本列表来自各家的官方元数据（Fabric 的 `meta.fabricmc.net`、Forge 与
 * NeoForge 的官方 maven）。这里只负责把结果铺开：不可用的行置灰并写明原因，可用的行
 * 点开是版本列表，选好版本才出现「安装」。
 *
 * 点「安装」走 `installModLoaderVersion`：后端会先装原版，再跑加载器自己的安装步骤，
 * 最后建一个指向加载器版本的实例。进度走 `install://progress` 事件。
 */
type ModLoaderPickerProps = {
  /** 要添加加载器的游戏版本，例如 `1.21.1`。 */
  gameVersion: string;
  /** 安装完成后把新实例交给调用方。 */
  onInstalled?: (instance: InstanceSummary) => void;
};

/** 加载器的固定顺序：PCL 的习惯是 Forge 在前，Fabric 在后。 */
const loaderOrder: readonly ModLoaderKind[] = ["forge", "neoforge", "fabric", "quilt"];

/** 「安装」按钮的状态，只属于展开的那一行。 */
type DownloadState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "done" }
  | { status: "failed"; message: string };

const idle: DownloadState = { status: "idle" };

/**
 * 中间那段状态文字。
 *
 * 可用且已经选了版本时直接显示选中的版本号 —— 这也是「选了显示在上面」的意思：
 * 收起列表以后，行上写的仍然是你要装的那个版本，不用再展开去找。
 */
function describeChoice(choice: ModLoaderChoice, selected: string | undefined): string {
  if (!choice.available) {
    return choice.reason ?? "暂不可用";
  }
  if (choice.versions.length === 0) {
    return "没有可选的版本";
  }
  return selected ?? "未选择版本";
}

/**
 * 列表里每个版本右边的标签。
 *
 * 只标「最新」：其余行再写「可选」没有信息量，列表本来就是可选的。
 */
function describeVersion(choice: ModLoaderChoice, version: string): string | null {
  return choice.versions[0] === version ? "最新" : null;
}

export function ModLoaderPicker({ gameVersion, onInstalled }: ModLoaderPickerProps) {
  // 用 ref 存回调，install() 就不会因为它变化而重建。
  const onInstalledRef = useRef(onInstalled);
  useEffect(() => {
    onInstalledRef.current = onInstalled;
  }, [onInstalled]);
  const desktop = isTauri();

  const [choices, setChoices] = useState<ModLoaderChoice[]>([]);
  const [loading, setLoading] = useState(desktop);
  // 递增它表示「再读一次」，effect 只依赖它，不在读取过程中同步写 state
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState<string | null>(null);

  /** 当前展开的加载器；同一时间只展开一行，和 PCL 一样。 */
  const [openKind, setOpenKind] = useState<ModLoaderKind | null>(null);
  /** 每个加载器各自选了哪个版本，收起再展开时保留。 */
  const [picked, setPicked] = useState<Partial<Record<ModLoaderKind, string>>>({});
  /** 下载状态跟着加载器走，切换行时不会把上一行的提示带过去。 */
  const [download, setDownload] = useState<Partial<Record<ModLoaderKind, DownloadState>>>({});

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

  const toggle = (choice: ModLoaderChoice) => {
    if (!choice.available) {
      return;
    }
    const opening = openKind !== choice.kind;
    setOpenKind(opening ? choice.kind : null);
    // 展开一个新加载器时默认选它最新的版本：绝大多数人就是装最新的，
    // 让他们再去列表里点一次纯属多余。已经选过的不覆盖。
    if (opening && choice.versions.length > 0) {
      setPicked((current) =>
        current[choice.kind] === undefined
          ? { ...current, [choice.kind]: choice.versions[0] }
          : current,
      );
    }
  };

  const pick = (kind: ModLoaderKind, version: string) => {
    setPicked((current) => ({ ...current, [kind]: version }));
    // 换版本后，上一条「已下载 / 失败」的提示就过期了
    setDownload((current) => ({ ...current, [kind]: idle }));
  };

  const install = async (choice: ModLoaderChoice) => {
    const version = picked[choice.kind];
    if (version === undefined || (download[choice.kind] ?? idle).status === "running") {
      return;
    }

    setDownload((current) => ({ ...current, [choice.kind]: { status: "running" } }));
    try {
      // 先装原版、再装加载器、最后把实例切到加载器：整条链在后端一次跑完。
      const result = await installModLoaderVersion(choice.kind, gameVersion, version);
      setDownload((current) => ({ ...current, [choice.kind]: { status: "done" } }));
      onInstalledRef.current?.(result.instance);
    } catch (cause) {
      setDownload((current) => ({
        ...current,
        [choice.kind]: { status: "failed", message: describeCommandError(cause) },
      }));
    }
  };

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
        <Text className="muted modloader-note" role="status">
          正在读取可用的加载器…
        </Text>
      ) : error !== null && choices.length === 0 ? (
        <div className="modloader-empty" role="alert">
          <Text className="modloader-error">{error}</Text>
          <Button appearance="secondary" onClick={retry}>
            重新读取
          </Button>
        </div>
      ) : (
        <ul className="modloader-list" aria-label="模组加载器">
          {choices.map((choice, index) => {
            const expanded = openKind === choice.kind;
            const selected = picked[choice.kind];
            const state = download[choice.kind] ?? idle;
            const canInstall = selected !== undefined && state.status !== "running";

            return (
              <li
                key={choice.kind}
                className="modloader-item"
                style={{ "--i": index } as CSSProperties}
                data-band={choice.kind}
              >
                <Tooltip
                  content={
                    choice.available
                      ? selected === undefined
                        ? `为 ${gameVersion} 添加 ${choice.label}`
                        : `${choice.label} ${selected}`
                      : describeChoice(choice, selected)
                  }
                  relationship="description"
                  withArrow
                  positioning="above-start"
                >
                  <button
                    type="button"
                    className="modloader-row"
                    aria-label={choice.label}
                    aria-expanded={choice.available ? expanded : undefined}
                    aria-controls={choice.available ? `modloader-versions-${choice.kind}` : undefined}
                    aria-disabled={!choice.available}
                    data-available={choice.available}
                    onClick={() => toggle(choice)}
                  >
                    <span className="modloader-name">
                      <span className="modloader-band" aria-hidden="true" />
                      <Text weight="semibold">{choice.label}</Text>
                    </span>
                    <span className="modloader-status">
                      {choice.available && choice.versions.length > 0 ? (
                        <CheckmarkCircle20Filled
                          className="modloader-status-icon"
                          aria-hidden="true"
                        />
                      ) : (
                        <Warning16Filled className="modloader-status-icon" aria-hidden="true" />
                      )}
                      <span className="modloader-status-text">
                        {describeChoice(choice, selected)}
                      </span>
                    </span>
                    {choice.available ? (
                      <ChevronRight16Regular
                        aria-hidden="true"
                        className={`row-chevron${expanded ? " expanded" : ""}`}
                      />
                    ) : (
                      <span className="modloader-chevron-placeholder" aria-hidden="true" />
                    )}
                  </button>
                </Tooltip>

                {expanded ? (
                  <div className="modloader-body" id={`modloader-versions-${choice.kind}`}>
                    {choice.versions.length === 0 ? (
                      <Text className="muted" size={200}>
                        这个加载器还没有发布支持 {gameVersion} 的版本。
                      </Text>
                    ) : (
                      <>
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
                                  onClick={() => pick(choice.kind, version)}
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

                        {choice.companions.map((companion) => (
                          <div className="modloader-companion" key={companion.label}>
                            <span className="modloader-companion-name">{companion.label}</span>
                            {companion.versions.length > 0 ? (
                              <>
                                <span className="modloader-companion-version">
                                  {companion.versions[0]}
                                </span>
                                <span className="modloader-companion-note">会一起装上</span>
                              </>
                            ) : (
                              <span className="modloader-companion-note">
                                {companion.reason ?? "读不到版本"}
                              </span>
                            )}
                          </div>
                        ))}
                        <div className="modloader-actions">
                          <Button
                            appearance="primary"
                            icon={<ArrowDownload20Regular />}
                            disabled={!canInstall}
                            onClick={() => void install(choice)}
                          >
                            {state.status === "running" ? "正在安装…" : "安装"}
                          </Button>
                          {selected === undefined ? (
                            <Text className="muted" size={200}>
                              先选一个版本。
                            </Text>
                          ) : (
                            <Text className="muted" size={200}>
                              将安装 {choice.label} {selected}：先装游戏本体，再装上加载器并新建一个实例。
                            </Text>
                          )}
                        </div>

                        {state.status === "done" ? (
                          <div className="modloader-feedback success" role="status">
                            <CheckmarkCircle20Filled aria-hidden="true" />
                            <span>
                              {choice.label} {selected} 已装好，新实例已经建好了。
                            </span>
                          </div>
                        ) : null}

                        {state.status === "failed" ? (
                          <div className="modloader-feedback failed" role="alert">
                            <Warning16Filled aria-hidden="true" />
                            <span>{state.message}</span>
                          </div>
                        ) : null}
                      </>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
