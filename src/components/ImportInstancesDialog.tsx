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
  Spinner,
  Tab,
  TabList,
  Text,
} from "@fluentui/react-components";
import {
  ArrowDownload24Regular,
  FolderOpen24Regular,
} from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import {
  describeCommandError,
  detectLauncherInstallations,
  importVersions,
  installImportedInstance,
  pickImportFolder,
  pickLauncherFolder,
} from "../ipc";
import type {
  ImportResult,
  ImportSource,
  InstanceSummary,
  LauncherInstallation,
} from "../ipc";
import {
  allCandidateIds,
  describeImportSkipReason,
  describeImportSourceKind,
  summariseImport,
  toggleAllCandidates,
  toggleCandidate,
} from "../lib/import";

type ImportInstancesDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 导入成功后回调，页面据此把新实例并入列表 */
  onImported: (instances: InstanceSummary[]) => void;
};

type ImportTab = "folder" | "launcher";

type Busy = "idle" | "picking" | "detecting" | "importing" | "installing";

/**
 * 导入实例对话框。
 *
 * 两个分支：
 * - **从文件夹导入**：后端打开系统文件夹选择框，扫描用户选中的 `.minecraft`
 *   或其他游戏目录；
 * - **从启动器导入**：后端在有限范围内查找 PCL2、HMCL、BakaXL、Prism Launcher
 *   和 MultiMC，读取各自配置里记录的游戏目录或实例；自动找不到时可以手动指定
 *   启动器目录。
 *
 * 前端从不传路径：预览返回的 `token` 是后端保存的扫描结果句柄，导入时只提交
 * `token` 和勾选的条目 ID。导入只复制版本元数据，游戏文件由「补齐游戏文件」
 * 走已有的下载流程（官方源失败时自动回退 BMCLAPI 镜像）。
 */
export function ImportInstancesDialog({
  open,
  onOpenChange,
  onImported,
}: ImportInstancesDialogProps) {
  const [tab, setTab] = useState<ImportTab>("folder");
  const [source, setSource] = useState<ImportSource | null>(null);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [installations, setInstallations] = useState<
    LauncherInstallation[] | null
  >(null);
  const [busy, setBusy] = useState<Busy>("idle");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [installNote, setInstallNote] = useState<string | null>(null);
  // Prism / MultiMC 不存 versions/<id>/<id>.json，补齐文件前要先从官方清单缓存
  const [needsManifest, setNeedsManifest] = useState(false);

  const reset = useCallback(() => {
    setTab("folder");
    setSource(null);
    setSelected(new Set());
    setInstallations(null);
    setBusy("idle");
    setError(null);
    setResult(null);
    setInstallNote(null);
    setNeedsManifest(false);
  }, []);

  const close = useCallback(() => {
    reset();
    onOpenChange(false);
  }, [onOpenChange, reset]);

  const detect = useCallback(async () => {
    setBusy("detecting");
    setError(null);
    try {
      setInstallations(await detectLauncherInstallations());
    } catch (failure: unknown) {
      setInstallations([]);
      setError(describeCommandError(failure));
    } finally {
      setBusy("idle");
    }
  }, []);

  // 切到启动器分支时才扫描，避免每次打开对话框都做一次磁盘探测
  const selectTab = (next: ImportTab) => {
    setTab(next);
    setError(null);
    if (next === "launcher" && installations === null && busy === "idle") {
      void detect();
    }
  };

  const chooseSource = (next: ImportSource) => {
    setSource(next);
    setSelected(new Set(allCandidateIds(next)));
    setResult(null);
    setInstallNote(null);
    setError(null);
  };

  const handlePickFolder = async () => {
    setBusy("picking");
    setError(null);
    try {
      const picked = await pickImportFolder();
      if (picked) {
        chooseSource(picked);
      }
    } catch (failure: unknown) {
      setError(describeCommandError(failure));
    } finally {
      setBusy("idle");
    }
  };

  const handlePickLauncherFolder = async () => {
    setBusy("picking");
    setError(null);
    try {
      const picked = await pickLauncherFolder();
      if (picked) {
        setInstallations([picked]);
        const [first] = picked.sources;
        if (first && picked.sources.length === 1) {
          chooseSource(first);
        }
      }
    } catch (failure: unknown) {
      setError(describeCommandError(failure));
    } finally {
      setBusy("idle");
    }
  };

  const handleImport = async () => {
    if (!source || selected.size === 0) {
      return;
    }
    setBusy("importing");
    setError(null);
    try {
      const imported = await importVersions(source.token, [...selected]);
      setResult(imported);
      setNeedsManifest(source.sourceKind === "prism");
      setSource(null);
      setSelected(new Set());
      if (imported.imported.length > 0) {
        onImported(imported.imported);
      }
    } catch (failure: unknown) {
      setError(describeCommandError(failure));
    } finally {
      setBusy("idle");
    }
  };

  const handleInstallFiles = async () => {
    if (!result) {
      return;
    }
    setBusy("installing");
    setInstallNote(null);
    let files = 0;
    const failures: string[] = [];
    for (const instance of result.imported) {
      try {
        files += await installImportedInstance(instance.id, needsManifest);
      } catch (failure: unknown) {
        failures.push(`${instance.version}：${describeCommandError(failure)}`);
      }
    }
    setInstallNote(
      failures.length === 0
        ? `已下载并校验 ${files} 个文件。`
        : `已下载并校验 ${files} 个文件；${failures.join("；")}`,
    );
    setBusy("idle");
  };

  const busyNow = busy !== "idle";
  const canImport = source !== null && selected.size > 0 && !busyNow;

  return (
    <Dialog
      open={open}
      onOpenChange={(_, data) => {
        if (!data.open) {
          close();
        }
      }}
    >
      <DialogSurface aria-describedby={undefined}>
        <DialogBody>
          <DialogTitle>导入实例</DialogTitle>
          <DialogContent className="dialog-content">
            <TabList
              selectedValue={tab}
              onTabSelect={(_, data) => selectTab(data.value as ImportTab)}
            >
              <Tab value="folder">从文件夹导入</Tab>
              <Tab value="launcher">从启动器导入</Tab>
            </TabList>

            {tab === "folder" ? (
              <section className="import-section">
                <Text size={200} className="muted">
                  选择现有的 <code>.minecraft</code> 文件夹，或任何含{" "}
                  <code>versions/&lt;版本&gt;/&lt;版本&gt;.json</code> 的游戏目录。
                  会自动识别 PCL2、HMCL、BakaXL 的目录结构。
                </Text>
                <Button
                  icon={<FolderOpen24Regular />}
                  disabled={busyNow}
                  onClick={() => void handlePickFolder()}
                >
                  {busy === "picking" ? "正在扫描…" : "选择 .minecraft 文件夹"}
                </Button>
              </section>
            ) : (
              <section className="import-section">
                <Text size={200} className="muted">
                  自动查找 PCL2、HMCL、BakaXL、Prism Launcher 和 MultiMC，并读取它们
                  配置里记录的游戏目录或实例。找不到时手动指定启动器所在的文件夹。
                </Text>
                <div className="import-actions">
                  <Button
                    icon={<FolderOpen24Regular />}
                    disabled={busyNow}
                    onClick={() => void handlePickLauncherFolder()}
                  >
                    {busy === "picking" ? "正在读取…" : "手动选择启动器文件夹"}
                  </Button>
                  <Button
                    appearance="secondary"
                    disabled={busyNow}
                    onClick={() => void detect()}
                  >
                    {busy === "detecting" ? "正在查找…" : "重新自动查找"}
                  </Button>
                </div>

                {busy === "detecting" ? (
                  <Spinner size="tiny" label="正在查找启动器…" />
                ) : installations === null ? null : installations.length ===
                  0 ? (
                  <Text size={200} className="muted">
                    没有自动找到启动器。可以手动选择启动器所在的文件夹。
                  </Text>
                ) : (
                  <ul className="import-installations">
                    {installations.map((installation) => (
                      <li key={`${installation.kind}:${installation.launcherRoot}`}>
                        <div className="import-source-heading">
                          <Badge appearance="tint">
                            {describeImportSourceKind(installation.kind)}
                          </Badge>
                          <Text size={200} className="muted">
                            {installation.launcherRoot}
                          </Text>
                        </div>
                        <div className="import-directories">
                          {installation.sources.map((directory) => (
                            <Button
                              key={directory.token}
                              size="small"
                              appearance={
                                source?.token === directory.token
                                  ? "primary"
                                  : "secondary"
                              }
                              disabled={busyNow}
                              onClick={() => chooseSource(directory)}
                            >
                              {directory.sourcePath}
                              <Badge appearance="outline">
                                {directory.candidates.length} 个
                                {directory.sourceKind === "prism"
                                  ? "实例"
                                  : "版本"}
                              </Badge>
                            </Button>
                          ))}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            )}

            {error ? (
              <MessageBar intent="error">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            ) : null}

            {source ? (
              <section className="import-section">
                <div className="import-source-heading">
                  <Text weight="semibold">
                    {describeImportSourceKind(source.sourceKind)}
                  </Text>
                  <Text size={200} className="muted">
                    {source.sourcePath}
                  </Text>
                </div>
                {source.sourceKind === "prism" ? (
                  <Text size={200} className="muted">
                    Prism / MultiMC 不保存版本 JSON，导入只创建基础版本实例；
                    Fabric、Forge 等加载器需要另行安装。
                  </Text>
                ) : null}
                <div className="import-actions">
                  <Button
                    size="small"
                    appearance="secondary"
                    disabled={busyNow}
                    onClick={() =>
                      setSelected(
                        toggleAllCandidates(selected, allCandidateIds(source)),
                      )
                    }
                  >
                    {selected.size === source.candidates.length
                      ? "全不选"
                      : "全选"}
                  </Button>
                  <Text size={200} className="muted">
                    已选 {selected.size} / {source.candidates.length}
                  </Text>
                </div>
                <ul className="import-candidates">
                  {source.candidates.map((candidate) => (
                    <li key={candidate.id}>
                      <Checkbox
                        checked={selected.has(candidate.id)}
                        disabled={busyNow}
                        label={candidate.displayName}
                        onChange={() =>
                          setSelected(toggleCandidate(selected, candidate.id))
                        }
                      />
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}

            {result ? (
              <section className="import-section">
                <Text weight="semibold">{summariseImport(result)}</Text>
                {result.skipped.length > 0 ? (
                  <ul className="skipped-list">
                    {result.skipped.map((item) => (
                      <li key={item.id}>
                        <Text className="muted">
                          {item.id}：{describeImportSkipReason(item.reason)}
                        </Text>
                      </li>
                    ))}
                  </ul>
                ) : null}
                {result.imported.length > 0 ? (
                  <>
                    <Text size={200} className="muted">
                      导入只复制版本元数据。客户端、依赖库和资源需要下载补齐，
                      官方源不可用时自动回退 BMCLAPI 镜像。
                    </Text>
                    <Button
                      icon={<ArrowDownload24Regular />}
                      disabled={busyNow}
                      onClick={() => void handleInstallFiles()}
                    >
                      {busy === "installing"
                        ? "正在下载并校验…"
                        : "下载并补齐游戏文件"}
                    </Button>
                    {installNote ? (
                      <Text size={200} role="status">
                        {installNote}
                      </Text>
                    ) : null}
                  </>
                ) : null}
              </section>
            ) : null}
          </DialogContent>

          <DialogActions>
            <Button appearance="secondary" onClick={close} disabled={busyNow}>
              关闭
            </Button>
            <Button
              appearance="primary"
              disabled={!canImport}
              onClick={() => void handleImport()}
            >
              {busy === "importing"
                ? "正在导入…"
                : `导入选中${selected.size > 0 ? `（${selected.size}）` : ""}`}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
