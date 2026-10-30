import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  MessageBar,
  MessageBarBody,
  Radio,
  RadioGroup,
  Text,
} from "@fluentui/react-components";
import { Delete20Regular, Edit20Regular, FolderOpen20Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import type { FormEvent } from "react";
import {
  deleteInstance,
  describeCommandError,
  getSystemMemory,
  openInstanceFolder,
  renameInstance,
} from "../ipc";
import type { InstanceDetails, InstanceSummary } from "../ipc";
import { formatBytes } from "../lib/instanceContent";
import { validateInstanceName } from "../lib/instances";
import {
  clearInstanceSettings,
  loadInstanceSettings,
  saveInstanceSettings,
} from "../lib/instanceSettings";
import type { InstanceSettings } from "../lib/instanceSettings";
import { formatMiB } from "../lib/memory";
import { useQuery } from "../lib/query";
import { loadMemoryMiB } from "../lib/settings";
import { useInstances } from "../state/instances";
import { ConfirmDialog } from "./ConfirmDialog";
import { MemoryBar } from "./MemoryBar";

type InstanceOverviewProps = {
  instance: InstanceSummary;
  details: InstanceDetails | null;
  /** 游戏正在运行：不能删除实例 */
  locked: boolean;
  /** 改名之后调用，让列表和标题跟着更新 */
  onRenamed: () => void;
  onDeleted: () => void;
};

/**
 * 概览：这个实例是什么、能对它做什么、启动时用多少内存。
 *
 * 内存默认跟随设置页里的全局选择；这里可以只为这个实例改。覆盖保存在本机
 * （`localStorage`），不进实例记录，删除实例时一并清掉。
 */
export function InstanceOverview({ instance, details, locked, onRenamed, onDeleted }: InstanceOverviewProps) {
  const { instances } = useInstances();
  const [notice, setNotice] = useState<string | null>(null);

  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(instance.name);
  const [renameError, setRenameError] = useState<string | null>(null);
  const [renameBusy, setRenameBusy] = useState(false);

  const [deleting, setDeleting] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const startRename = () => {
    setDraft(instance.name);
    setRenameError(null);
    setRenaming(true);
  };

  const duplicate = instances.some(
    (other) => other.id !== instance.id && other.name.toLowerCase() === draft.trim().toLowerCase(),
  );
  const nameProblem = validateInstanceName(draft) ?? (duplicate ? "已经有同名实例了" : null);

  const submitRename = async (event: FormEvent) => {
    event.preventDefault();
    if (nameProblem !== null) {
      setRenameError(nameProblem);
      return;
    }
    setRenameBusy(true);
    try {
      await renameInstance(instance.id, draft.trim());
      setRenaming(false);
      onRenamed();
    } catch (failure: unknown) {
      setRenameError(describeCommandError(failure));
    } finally {
      setRenameBusy(false);
    }
  };

  const confirmDelete = async () => {
    setDeleteBusy(true);
    setDeleteError(null);
    try {
      await deleteInstance(instance.id);
      clearInstanceSettings(instance.id);
      setDeleting(false);
      onDeleted();
    } catch (failure: unknown) {
      setDeleteError(describeCommandError(failure));
    } finally {
      setDeleteBusy(false);
    }
  };

  const openGameFolder = async () => {
    setNotice(null);
    try {
      await openInstanceFolder(instance.id, "game");
    } catch (failure: unknown) {
      setNotice(describeCommandError(failure));
    }
  };

  return (
    <div className="detail-grid">
      <div className="detail-main">
        {notice ? (
          <MessageBar intent="error" className="page-notice" role="alert">
            <MessageBarBody>{notice}</MessageBarBody>
          </MessageBar>
        ) : null}

        <section className="status-panel setting-group" aria-labelledby="launch-settings-heading">
          <Text id="launch-settings-heading" as="h2" size={400} weight="semibold">
            启动设置
          </Text>
          <LaunchSettings instanceId={instance.id} />
        </section>

        <section className="status-panel setting-group" aria-labelledby="actions-heading">
          <Text id="actions-heading" as="h2" size={400} weight="semibold">
            管理
          </Text>
          <div className="manage-actions">
            <Button icon={<Edit20Regular />} onClick={startRename}>
              重命名
            </Button>
            <Button icon={<FolderOpen20Regular />} onClick={() => void openGameFolder()}>
              打开游戏文件夹
            </Button>
            <Button
              icon={<Delete20Regular />}
              disabled={locked}
              onClick={() => {
                setDeleteError(null);
                setDeleting(true);
              }}
            >
              删除实例
            </Button>
          </div>
          <Text size={200} className="muted">
            {locked
              ? "游戏正在运行，停止游戏之后才能删除这个实例。"
              : "删除只会删掉这个实例自己的文件（存档、模组、设置）。其他实例和共享的游戏文件不受影响。"}
          </Text>
        </section>
      </div>

      <aside className="detail-side" aria-label="实例信息">
        <section className="status-panel">
          <Text as="h2" size={400} weight="semibold">
            实例信息
          </Text>
          <dl className="meta-list">
            <div>
              <dt>名称</dt>
              <dd>{instance.name}</dd>
            </div>
            <div>
              <dt>游戏版本</dt>
              <dd>{instance.version}</dd>
            </div>
            <div>
              <dt>模组加载器</dt>
              <dd>{details === null ? "…" : (details.loader ?? "无（原版）")}</dd>
            </div>
            <div>
              <dt>占用空间</dt>
              <dd>{details === null ? "…" : formatBytes(details.sizeBytes)}</dd>
            </div>
            <div>
              <dt>存档</dt>
              <dd>{details === null ? "…" : `${details.counts.saves} 个`}</dd>
            </div>
            <div>
              <dt>模组</dt>
              <dd>{details === null ? "…" : `${details.counts.mods} 个`}</dd>
            </div>
          </dl>
          <Text size={200} className="muted">
            占用空间只算这个实例自己的文件，共享的游戏文件、依赖库和资源不计入。
          </Text>
        </section>
      </aside>

      <Dialog open={renaming} onOpenChange={(_, data) => !renameBusy && setRenaming(data.open)}>
        <DialogSurface aria-describedby={undefined}>
          <form onSubmit={(event) => void submitRename(event)}>
            <DialogBody>
              <DialogTitle>重命名实例</DialogTitle>
              <DialogContent className="dialog-content">
                <Field
                  label="实例名称"
                  hint="只改显示的名字，文件夹和存档的位置不变。"
                  validationState={renameError !== null || nameProblem !== null ? "error" : "none"}
                  validationMessage={renameError ?? nameProblem ?? undefined}
                >
                  <Input
                    value={draft}
                    autoFocus
                    onChange={(_, data) => {
                      setDraft(data.value);
                      setRenameError(null);
                    }}
                  />
                </Field>
              </DialogContent>
              <DialogActions>
                <Button appearance="secondary" disabled={renameBusy} onClick={() => setRenaming(false)}>
                  取消
                </Button>
                <Button
                  type="submit"
                  appearance="primary"
                  disabled={renameBusy || nameProblem !== null || draft.trim() === instance.name}
                >
                  保存
                </Button>
              </DialogActions>
            </DialogBody>
          </form>
        </DialogSurface>
      </Dialog>

      <ConfirmDialog
        open={deleting}
        title="删除这个实例？"
        body={
          <Text>
            「{instance.name}」的存档、模组、资源包、截图和设置都会被删除，无法恢复。
            {details !== null && details.sizeBytes > 0 ? `（约 ${formatBytes(details.sizeBytes)}）` : ""}
          </Text>
        }
        confirmLabel="删除实例"
        danger
        busy={deleteBusy}
        error={deleteError}
        onConfirm={() => void confirmDelete()}
        onCancel={() => setDeleting(false)}
      />
    </div>
  );
}

/** 这个实例的内存：默认跟随全局设置，可以只为这一个实例改。 */
function LaunchSettings({ instanceId }: { instanceId: string }) {
  const [settings, setSettings] = useState<InstanceSettings>(() => loadInstanceSettings(instanceId));
  const { data: system } = useQuery(getSystemMemory);

  const update = useCallback(
    (next: InstanceSettings) => {
      setSettings(saveInstanceSettings(instanceId, next));
    },
    [instanceId],
  );

  const custom = settings.memoryMiB !== null;

  return (
    <div className="setting-row stacked">
      <div className="setting-copy">
        <Text weight="semibold">内存上限</Text>
        <Text size={200} className="muted">
          只对这个实例生效。选「跟随全局设置」就用设置页里的自动或自定义值。
        </Text>
      </div>
      <RadioGroup
        aria-label="这个实例的内存"
        value={custom ? "custom" : "inherit"}
        onChange={(_, data) => update({ memoryMiB: data.value === "custom" ? loadMemoryMiB() : null })}
      >
        <Radio value="inherit" label="跟随全局设置" />
        <Radio value="custom" label="只为这个实例设置" />
      </RadioGroup>
      {settings.memoryMiB !== null ? (
        <>
          <MemoryBar
            system={system}
            valueMiB={settings.memoryMiB}
            onChange={(mib) => update({ memoryMiB: mib })}
          />
          <Text size={200} className="muted">
            启动时给游戏 {formatMiB(settings.memoryMiB)}。
          </Text>
        </>
      ) : null}
    </div>
  );
}
