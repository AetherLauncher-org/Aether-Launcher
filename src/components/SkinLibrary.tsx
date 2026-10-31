import {
  Badge,
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
  Spinner,
  Switch,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import { Add20Regular, Delete20Regular, Edit20Regular, Person20Regular } from "@fluentui/react-icons";
import { useCallback, useState } from "react";
import {
  addSkinFromFile,
  addSkinFromPlayer,
  describeCommandError,
  listSkins,
  removeSkin,
  renameSkin,
  setSkinModel,
} from "../ipc";
import type { AddSkinResult, SkinEntry } from "../ipc";
import { useQuery } from "../lib/query";
import { ConfirmDialog } from "./ConfirmDialog";
import { EmptyState } from "./EmptyState";
import { ErrorState } from "./PageState";
import { SkinFigure } from "./SkinFigure";

const sourceLabels: Record<SkinEntry["source"], string> = {
  file: "文件",
  player: "玩家",
  download: "下载",
};

/**
 * 皮肤库：本机保存的皮肤，每张画成正对着看的人。可以从文件（系统对话框选）或玩家名添加，
 * 改名、选纤细或经典手臂、删除。同一张图加两次只有一张。
 *
 * 「换上」还没接到账户上（换肤要用微软账户的令牌，只能在后端里用），按钮先灰着并说明。
 */
export function SkinLibrary() {
  const load = useCallback(() => listSkins(), []);
  const { status, data, error, reload } = useQuery(load);

  const [notice, setNotice] = useState<{ intent: "success" | "info" | "error"; text: string } | null>(null);
  const [adding, setAdding] = useState(false);
  const [byName, setByName] = useState(false);
  const [renaming, setRenaming] = useState<SkinEntry | null>(null);
  const [removing, setRemoving] = useState<SkinEntry | null>(null);

  const skins = data ?? [];

  const report = (result: AddSkinResult) => {
    setNotice(
      result.added
        ? { intent: "success", text: `已加进皮肤库：${result.entry.name}` }
        : { intent: "info", text: `皮肤库里已经有这张皮肤了：${result.entry.name}` },
    );
    reload();
  };

  const addFile = async () => {
    setNotice(null);
    setAdding(true);
    try {
      const result = await addSkinFromFile();
      if (result !== null) {
        report(result);
      }
    } catch (failure: unknown) {
      setNotice({ intent: "error", text: describeCommandError(failure) });
    } finally {
      setAdding(false);
    }
  };

  const toggleModel = async (skin: SkinEntry, slim: boolean) => {
    setNotice(null);
    try {
      await setSkinModel(skin.id, slim);
      reload();
    } catch (failure: unknown) {
      setNotice({ intent: "error", text: describeCommandError(failure) });
    }
  };

  return (
    <section className="skin-library" aria-label="皮肤库">
      <div className="toolbar">
        <Text className="muted">{status === "success" ? `共 ${skins.length} 张` : " "}</Text>
        <div className="toolbar-actions">
          <Button appearance="secondary" icon={<Person20Regular />} onClick={() => setByName(true)}>
            从玩家名添加
          </Button>
          <Button
            appearance="primary"
            icon={adding ? <Spinner size="tiny" /> : <Add20Regular />}
            disabled={adding}
            onClick={() => void addFile()}
          >
            添加皮肤文件
          </Button>
        </div>
      </div>

      {notice ? (
        <MessageBar intent={notice.intent} className="page-notice" role={notice.intent === "error" ? "alert" : "status"}>
          <MessageBarBody>{notice.text}</MessageBarBody>
        </MessageBar>
      ) : null}

      {status === "error" && error ? (
        <ErrorState title="读取皮肤库失败" message={error} onRetry={reload} retryLabel="重新读取" />
      ) : data === null ? (
        <div className="loading-block" role="status">
          <Spinner label="正在读取皮肤库…" />
        </div>
      ) : skins.length === 0 ? (
        <EmptyState
          title="皮肤库是空的"
          body="添加一张 PNG 皮肤文件，或者输入一个玩家名，把他现在穿的皮肤存下来。"
        />
      ) : (
        <ul className="skin-grid">
          {skins.map((skin) => (
            <li className="skin-card" key={skin.id}>
              <div className="skin-card-figure">
                <SkinFigure skinId={skin.id} slim={skin.slim} />
              </div>
              <div className="skin-card-body">
                <Text weight="semibold" truncate wrap={false} title={skin.name}>
                  {skin.name}
                </Text>
                <div className="content-card-facts">
                  <Badge appearance="tint" color="informative">
                    {sourceLabels[skin.source]}
                  </Badge>
                  <Switch
                    label="纤细手臂"
                    checked={skin.slim}
                    onChange={(_, state) => void toggleModel(skin, state.checked)}
                    aria-label={`${skin.name} 用纤细手臂`}
                  />
                </div>
                <div className="content-card-actions">
                  <Tooltip content="换肤要用微软账户登录，这一步还没接上" relationship="description">
                    <Button size="small" appearance="primary" disabledFocusable aria-label={`换上 ${skin.name}`}>
                      换上
                    </Button>
                  </Tooltip>
                  <div className="content-card-buttons">
                    <Button
                      size="small"
                      appearance="subtle"
                      icon={<Edit20Regular />}
                      aria-label={`重命名 ${skin.name}`}
                      onClick={() => setRenaming(skin)}
                    />
                    <Button
                      size="small"
                      appearance="subtle"
                      icon={<Delete20Regular />}
                      aria-label={`删除 ${skin.name}`}
                      onClick={() => setRemoving(skin)}
                    />
                  </div>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      <NameDialog
        open={byName}
        title="从玩家名添加"
        label="玩家名"
        hint="取这个玩家现在穿的皮肤（从 Mojang 官方服务），不需要登录。"
        confirmLabel="添加"
        onClose={() => setByName(false)}
        onConfirm={async (name) => {
          report(await addSkinFromPlayer(name));
          setByName(false);
        }}
      />
      <NameDialog
        key={renaming?.id ?? "none"}
        open={renaming !== null}
        title="重命名皮肤"
        label="名字"
        initial={renaming?.name ?? ""}
        confirmLabel="保存"
        onClose={() => setRenaming(null)}
        onConfirm={async (name) => {
          if (renaming !== null) {
            await renameSkin(renaming.id, name);
            reload();
          }
          setRenaming(null);
        }}
      />
      <ConfirmDialog
        open={removing !== null}
        title="从皮肤库删除这张皮肤？"
        body={<Text>「{removing?.name}」会从皮肤库里删掉。已经换在账户上的不受影响。</Text>}
        confirmLabel="删除"
        danger
        onConfirm={() => {
          const skin = removing;
          setRemoving(null);
          if (skin !== null) {
            void removeSkin(skin.id).then(reload, (failure: unknown) =>
              setNotice({ intent: "error", text: describeCommandError(failure) }),
            );
          }
        }}
        onCancel={() => setRemoving(null)}
      />
    </section>
  );
}

type NameDialogProps = {
  open: boolean;
  title: string;
  label: string;
  hint?: string;
  initial?: string;
  confirmLabel: string;
  onClose: () => void;
  /** 抛错就留在对话框里显示原因 */
  onConfirm: (value: string) => Promise<void>;
};

/** 输入一个名字的小对话框；确认失败时把后端的原因显示在里面。 */
function NameDialog({ open, title, label, hint, initial = "", confirmLabel, onClose, onConfirm }: NameDialogProps) {
  const [value, setValue] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setFailure(null);
    try {
      await onConfirm(value.trim());
    } catch (error: unknown) {
      setFailure(describeCommandError(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(_, next) => {
        if (!next.open && !busy) {
          setFailure(null);
          onClose();
        }
      }}
    >
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{title}</DialogTitle>
          <DialogContent className="dialog-content">
            <Field label={label} hint={hint}>
              <Input
                value={value}
                maxLength={32}
                onChange={(_, data) => setValue(data.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && value.trim() !== "" && !busy) {
                    void submit();
                  }
                }}
              />
            </Field>
            {failure ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{failure}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" disabled={busy} onClick={onClose}>
              取消
            </Button>
            <Button
              appearance="primary"
              disabled={busy || value.trim() === ""}
              icon={busy ? <Spinner size="tiny" /> : undefined}
              onClick={() => void submit()}
            >
              {confirmLabel}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
