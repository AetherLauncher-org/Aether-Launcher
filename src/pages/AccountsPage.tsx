import {
  Badge,
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add20Regular,
  Checkmark16Filled,
  Delete20Regular,
  Person24Regular,
  ShieldLock24Regular,
} from "@fluentui/react-icons";
import { useState } from "react";
import type { CSSProperties } from "react";
import { AccountAvatar } from "../components/AccountAvatar";
import { AddOfflineAccountDialog } from "../components/AddOfflineAccountDialog";
import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { describeLoginStatus, startMicrosoftLogin } from "../ipc";
import type { MicrosoftLoginStatus } from "../ipc";
import { describeUuidMode, formatUuid, summariseAccounts } from "../lib/accounts";
import type { OfflineAccount } from "../lib/accounts";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";

type AccountsPageProps = {
  page: PageDefinition;
};

/**
 * 账户页。离线账户保存在本机并在重启后保留；点「添加」弹出对话框，
 * 在里面填用户名并选择 UUID 格式。微软正版登录的流程还没接通，
 * 按钮会如实说明状态，而不是点了没反应。界面不显示任何令牌。
 */
export function AccountsPage({ page }: AccountsPageProps) {
  const { book, active, select, remove } = useAccounts();
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<OfflineAccount | null>(null);
  const [loginStatus, setLoginStatus] = useState<MicrosoftLoginStatus | null>(null);
  const [loginPending, setLoginPending] = useState(false);

  const handleMicrosoft = async () => {
    setLoginPending(true);
    setLoginStatus(await startMicrosoftLogin());
    setLoginPending(false);
  };

  const addButton = (
    <Button appearance="primary" icon={<Add20Regular />} onClick={() => setAdding(true)}>
      添加离线账户
    </Button>
  );

  return (
    <>
      <PageHeader title={page.label} description={page.summary} action={addButton} />

      <section aria-labelledby="accounts-heading" className="section">
        <div className="section-heading">
          <Text id="accounts-heading" as="h2" size={500} weight="semibold">
            本机账户
          </Text>
          <Text className="muted">{summariseAccounts(book.accounts)}</Text>
        </div>

        {book.accounts.length === 0 ? (
          <EmptyState
            icon={<Person24Regular />}
            title="还没有账户"
            body="添加一个离线账户，就可以启动游戏了。离线账户只在本机使用，适合单机和局域网。"
            action={addButton}
          />
        ) : (
          <ul className="account-grid">
            {book.accounts.map((account, index) => {
              const isActive = active?.id === account.id;
              return (
                <li
                  key={account.id}
                  className={`account-card${isActive ? " active" : ""}`}
                  style={{ "--i": index } as CSSProperties}
                >
                  <AccountAvatar name={account.name} size={48} />
                  <div className="account-card-body">
                    <div className="account-card-title">
                      <Text weight="semibold" size={400} truncate wrap={false}>
                        {account.name}
                      </Text>
                      <Badge appearance="outline" size="small">
                        离线
                      </Badge>
                      {isActive ? (
                        <Badge appearance="tint" color="brand" size="small">
                          当前使用
                        </Badge>
                      ) : null}
                    </div>
                    <Text size={200} className="muted">
                      UUID：{describeUuidMode(account.uuidMode).label}
                    </Text>
                    {account.uuid ? (
                      <Text size={200} className="muted mono account-uuid" title={formatUuid(account.uuid)}>
                        {formatUuid(account.uuid)}
                      </Text>
                    ) : null}
                  </div>
                  <div className="account-card-actions">
                    {isActive ? (
                      <Button size="small" appearance="subtle" icon={<Checkmark16Filled />} disabled>
                        使用中
                      </Button>
                    ) : (
                      <Button size="small" appearance="secondary" onClick={() => select(account.id)}>
                        使用此账户
                      </Button>
                    )}
                    <Tooltip content="移除此账户" relationship="label">
                      <Button
                        size="small"
                        appearance="subtle"
                        icon={<Delete20Regular />}
                        aria-label={`移除 ${account.name}`}
                        onClick={() => setRemoving(account)}
                      />
                    </Tooltip>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-labelledby="microsoft-heading" className="section">
        <div className="section-heading">
          <Text id="microsoft-heading" as="h2" size={500} weight="semibold">
            微软账户
          </Text>
          <Badge appearance="tint" color="informative">
            正版
          </Badge>
        </div>
        <div className="status-panel microsoft-card">
          <Text className="muted">
            使用微软账户登录，可以进入正版服务器。令牌只会保存在本机的系统凭据库里，不会显示在界面上。
          </Text>
          <div>
            <Button appearance="secondary" onClick={() => void handleMicrosoft()} disabled={loginPending}>
              {loginPending ? "请稍候…" : "使用微软账户登录"}
            </Button>
          </div>
          <div className="inline-note" role="status">
            <ShieldLock24Regular aria-hidden="true" />
            <Text className="muted">
              {loginStatus ? describeLoginStatus(loginStatus) : "还没有登录。登录流程完成后这里会显示账户名。"}
            </Text>
          </div>
        </div>
      </section>

      <AddOfflineAccountDialog open={adding} onOpenChange={setAdding} />

      <Dialog open={removing !== null} onOpenChange={(_, data) => !data.open && setRemoving(null)}>
        <DialogSurface aria-describedby={undefined}>
          <DialogBody>
            <DialogTitle>移除账户「{removing?.name}」？</DialogTitle>
            <DialogContent>
              {removing?.uuidMode === "standard" ? (
                <Text>之后可以用同样的用户名重新添加，UUID 不会变，存档不受影响。</Text>
              ) : (
                <Text>
                  这个账户的 UUID 只保存在这里，移除后无法找回。如果存档依赖它，请先记下 UUID：
                  <span className="mono"> {removing?.uuid ? formatUuid(removing.uuid) : ""}</span>
                </Text>
              )}
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" onClick={() => setRemoving(null)}>
                取消
              </Button>
              <Button
                appearance="primary"
                onClick={() => {
                  if (removing) {
                    remove(removing.id);
                  }
                  setRemoving(null);
                }}
              >
                移除
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </>
  );
}
