import { Button, Text, Tooltip } from "@fluentui/react-components";
import {
  ArrowDownload20Regular,
  CheckmarkCircle20Filled,
  Dismiss16Regular,
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
 * 优先级和 PCL 一样：**先说不兼容**（`LoadForgeGetError` 里第一件事就是查
 * `SelectedXxx`），再说不可用的原因，最后才是「可以添加」/已选版本。
 */
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
  /** 每个加载器各自选了哪个版本；收起之后仍然显示。 */
  const [picked, setPicked] = useState<Partial<Record<ModLoaderKind, string>>>({});
  /** Fabric API 单独一张卡，跟着 Fabric 出现/消失。 */
  const [pickedCompanion, setPickedCompanion] = useState<string | null>(null);
  /** 让 Fabric API 只自动选一次，用户取消之后别再自动选回来。 */
  const [companionAutoPicked, setCompanionAutoPicked] = useState(false);
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

  /**
   * 某个加载器是不是被别的**已选**加载器挡着。
   *
   * 这是 PCL 的规则（`LoadForgeGetError` 里的
   * `If SelectedFabric IsNot Nothing Then Return "与 Fabric 不兼容"`）：
   * 不是一开始就把四个都标红，而是选了谁，才把与它互斥的项标出来。
   */
  const conflictWith = (choice: ModLoaderChoice): string | null => {
    if (!choice.available) {
      return null;
    }
    const chosen = choices.find(
      (other) => other.kind !== choice.kind && picked[other.kind] !== undefined,
    );
    if (chosen === undefined) {
      return null;
    }
    // Forge 与 NeoForge 同族、Fabric 与 Quilt 同族：族内可以共存，由用户自己二选一。
    const sameFamily =
      ["forge", "neoforge"].includes(choice.kind) ===
      ["forge", "neoforge"].includes(chosen.kind);
    return sameFamily ? null : `与 ${chosen.label} 不兼容`;
  };

  const toggle = (choice: ModLoaderChoice) => {
    // 被别的已选加载器挡住的项不给展开：和 PCL 的 `CardForge_PreviewSwap`
    // （`If LoadForgeGetError() IsNot Nothing Then e.Handled = True`）一样。
    if (!choice.available || conflictWith(choice) !== null) {
      return;
    }
    setOpenKind((current) => (current === choice.kind ? null : choice.kind));
  };

  /**
   * 选中一个版本，然后**把列表收回去**。
   *
   * 这是 PCL 的做法（`Forge_Selected` 里 `CardForge.IsSwapped = True`）：列表是
   * 一次性的，选完就该看到行上的结果，而不是继续占着版面。
   */
  const pick = (choice: ModLoaderChoice, version: string) => {
    setPicked((current) => ({ ...current, [choice.kind]: version }));
    setDownload((current) => ({ ...current, [choice.kind]: idle }));
    setOpenKind(null);

    // 选上 Fabric 之后 Fabric API 那张卡才出现，并默认选最新的一个版本
    // （PCL 的 `AutoSelectedFabricApi` 只自动选一次）。
    if (choice.kind === "fabric" && !companionAutoPicked) {
      const api = choice.companions.find((companion) => companion.label === "Fabric API");
      if (api !== undefined && api.versions.length > 0) {
        setPickedCompanion(api.versions[0]);
        setCompanionAutoPicked(true);
      }
    }
  };

  /** 点 X 清掉一个加载器的选择；清 Fabric 时连带清掉 Fabric API 那张卡。 */
  const clear = (kind: ModLoaderKind) => {
    setPicked((current) => {
      const next = { ...current };
      delete next[kind];
      return next;
    });
    setDownload((current) => ({ ...current, [kind]: idle }));
    setOpenKind((current) => (current === kind ? null : current));
    if (kind === "fabric") {
      setPickedCompanion(null);
      setCompanionAutoPicked(false);
    }
  };

  /** 取消选择 Fabric API，但保留 Fabric 本身。 */
  const clearCompanion = () => {
    setPickedCompanion(null);
    // 用户明确取消了，就别再自动帮他选回来。
    setCompanionAutoPicked(true);
  };

  const fabricApi = choices
    .find((choice) => choice.kind === "fabric")
    ?.companions.find((companion) => companion.label === "Fabric API");
  const fabricChosen = picked.fabric !== undefined;

  /** 当前选中的那个加载器（同一时间只会有一个）。 */
  const chosen = choices.find((choice) => picked[choice.kind] !== undefined);
  /** 有安装任务在跑时，整张卡片的按钮都停用。 */
  const anyRunning = Object.values(download).some((entry) => entry?.status === "running");
  const canInstallAny = chosen !== undefined && !anyRunning;
  /** 失败提示也放在列表外面，列表收起后仍然看得到。 */
  const failed = chosen === undefined ? undefined : download[chosen.kind];
  const failedMessage = failed?.status === "failed" ? failed.message : null;

  const install = async (choice: ModLoaderChoice | undefined) => {
    if (choice === undefined) {
      return;
    }
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
            // 「不兼容」只在选了别的加载器之后才出现，和 PCL 的 SelectedXxx 一致。
            const conflict = conflictWith(choice);
            const status = describeChoice(choice, selected, conflict);
            // 被挡住的项不允许展开，也不允许安装。
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
                    conflict ?? (selected === undefined
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
                        onClick={() => clear(choice.kind)}
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
                      </>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}

      {/*
        Fabric API 是单独一张卡：只有选了 Fabric 才出现，出现时默认选中最新版。
        这是 PCL 的 `CardFabricApi`（`If SelectedFabric Is Nothing Then
        CardFabricApi.Visibility = Visibility.Collapsed`）。
      */}
      {fabricChosen && fabricApi !== undefined ? (
        <div className="modloader-companion" data-available={pickedCompanion !== null}>
          <div className="modloader-companion-head">
            <span className="modloader-companion-name">Fabric API</span>
            {pickedCompanion === null ? (
              <span className="modloader-companion-note">
                {fabricApi.reason ?? "没有单独选中，不会自动装上"}
              </span>
            ) : (
              <span className="modloader-companion-version">{pickedCompanion}</span>
            )}
            {pickedCompanion === null ? null : (
              <button
                type="button"
                className="modloader-clear"
                aria-label="取消选择 Fabric API"
                onClick={clearCompanion}
              >
                <Dismiss16Regular aria-hidden="true" />
              </button>
            )}
          </div>
        </div>
      ) : null}

      {/*
        底部常驻的「开始安装」。PCL 的 `BtnStart`（Text="开始下载"）也在卡片列表外面，
        不在展开区域里 —— 列表收起之后它必须还在，否则选完版本就再也点不到。
      */}
      {desktop && !loading && choices.length > 0 ? (
        <div className="modloader-actions">
          <Button
            appearance="primary"
            icon={<ArrowDownload20Regular />}
            disabled={!canInstallAny}
            onClick={() => void install(chosen)}
          >
            {anyRunning ? "正在安装…" : "开始安装"}
          </Button>
          <Text className="muted" size={200}>
            {chosen === undefined
              ? "先选一个加载器版本。"
              : `将安装 ${chosen.label} ${picked[chosen.kind]}${
                  pickedCompanion === null ? "" : "（含 Fabric API）"
                }：先装游戏本体，再装上加载器并新建一个实例。`}
          </Text>
        </div>
      ) : null}

      {/* 安装结果也在列表外面：列表收起后这里是唯一的反馈位置。 */}
      {chosen !== undefined && (download[chosen.kind]?.status ?? "idle") === "done" ? (
        <div className="modloader-feedback success" role="status">
          <CheckmarkCircle20Filled aria-hidden="true" />
          <span>
            {chosen.label} {picked[chosen.kind]} 已装好，新实例已经建好了。
          </span>
        </div>
      ) : null}

      {failedMessage === null ? null : (
        <div className="modloader-feedback failed" role="alert">
          <Warning16Filled aria-hidden="true" />
          <span>{failedMessage}</span>
        </div>
      )}
    </section>
  );
}
