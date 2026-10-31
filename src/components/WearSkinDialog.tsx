import {
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
  Text,
} from "@fluentui/react-components";
import { useCallback, useState } from "react";
import { describeCommandError, listMicrosoftAccounts, wearSkin } from "../ipc";
import type { SkinEntry } from "../ipc";
import { useQuery } from "../lib/query";

type Props = {
  skin: SkinEntry;
  onClose: () => void;
  /** 至少换成功了一个账户 */
  onWorn: (names: string[]) => void;
};

/**
 * 「换上」：选要换皮肤的微软账户（可多选）。离线账户没有皮肤服务，不在列表里。
 * 只有一个账户时默认选中它；换肤用的令牌只在后端里用，这里只传皮肤和账户的 id。
 * 一个账户失败（要重新登录、Mojang 不接受等）不影响其他账户，原因逐个显示。
 */
export function WearSkinDialog({ skin, onClose, onWorn }: Props) {
  const load = useCallback(async () => (await listMicrosoftAccounts()) ?? [], []);
  const { status, data, error } = useQuery(load);
  const [chosen, setChosen] = useState<ReadonlySet<string> | null>(null);
  const [busy, setBusy] = useState(false);
  const [failures, setFailures] = useState<{ name: string; reason: string }[]>([]);

  const accounts = data ?? [];
  // 只有一个账户时，没动过选择就当它选中
  const selected = chosen ?? new Set(accounts.length === 1 ? accounts.map((account) => account.id) : []);

  const toggle = (id: string, on: boolean) => {
    const next = new Set(selected);
    if (on) {
      next.add(id);
    } else {
      next.delete(id);
    }
    setChosen(next);
  };

  const submit = async () => {
    setBusy(true);
    setFailures([]);
    const worn: string[] = [];
    const failed: { name: string; reason: string }[] = [];
    for (const account of accounts.filter((item) => selected.has(item.id))) {
      try {
        await wearSkin(skin.id, account.id);
        worn.push(account.displayName);
      } catch (failure: unknown) {
        failed.push({ name: account.displayName, reason: describeCommandError(failure) });
      }
    }
    setBusy(false);
    if (worn.length > 0) {
      onWorn(worn);
    }
    if (failed.length > 0) {
      setFailures(failed);
    } else {
      onClose();
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(_, next) => {
        if (!next.open && !busy) {
          onClose();
        }
      }}
    >
      <DialogSurface>
        <DialogBody>
          <DialogTitle>换上「{skin.name}」</DialogTitle>
          <DialogContent className="dialog-content">
            {status === "error" ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            ) : data === null ? (
              <Spinner label="正在读取账户…" />
            ) : accounts.length === 0 ? (
              <Text>还没有登录微软账户。换肤要用微软账户，离线账户没有皮肤服务。先到「账户」页登录一个。</Text>
            ) : (
              <>
                <Text className="muted">选要换上这张皮肤的账户（官方账户的皮肤会立刻改变）：</Text>
                <ul className="wear-accounts">
                  {accounts.map((account) => (
                    <li key={account.id}>
                      <Checkbox
                        checked={selected.has(account.id)}
                        disabled={busy}
                        label={account.displayName}
                        onChange={(_, state) => toggle(account.id, state.checked === true)}
                      />
                    </li>
                  ))}
                </ul>
              </>
            )}
            {failures.map((item) => (
              <MessageBar key={item.name} intent="error" role="alert">
                <MessageBarBody>
                  {item.name}：{item.reason}
                </MessageBarBody>
              </MessageBar>
            ))}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" disabled={busy} onClick={onClose}>
              {failures.length > 0 ? "关闭" : "取消"}
            </Button>
            <Button
              appearance="primary"
              disabled={busy || selected.size === 0}
              icon={busy ? <Spinner size="tiny" /> : undefined}
              onClick={() => void submit()}
            >
              换上
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
