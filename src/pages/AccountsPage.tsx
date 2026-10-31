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
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { AccountAvatar } from "../components/AccountAvatar";
import { AddOfflineAccountDialog } from "../components/AddOfflineAccountDialog";
import { EmptyState } from "../components/EmptyState";
import { MicrosoftLoginDialog } from "../components/MicrosoftLoginDialog";
import { PageHeader } from "../components/PageHeader";
import {
  describeCommandError,
  isMicrosoftLoginAvailable,
  isTauri,
  removeMicrosoftAccount,
} from "../ipc";
import { describeUuidMode, formatUuid, summariseAccounts } from "../lib/accounts";
import type { Account } from "../lib/accounts";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";

type AccountsPageProps = {
  page: PageDefinition;
};

/**
 * 账户页。离线账户保存在本机并在重启后保留；点「添加」弹出对话框，
 * 在里面填用户名并选择 UUID 格式。微软账户走设备码登录：令牌由后端保管，
 * 这里只显示账户身份。没有配置 Azure 应用 ID 的构建会如实说明，而不是点了没反应。
 */
export function AccountsPage({ page }: AccountsPageProps) {
  const { book, active, select, remove, signInMicrosoft } = useAccounts();
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<Account | null>(null);
  const [removeError, setRemoveError] = useState<string | null>(null);
  const [loginOpen, setLoginOpen] = useState(false);
  const [loginAvailable, setLoginAvailable] = useState<boolean | null>(null);

  useEffect(() => {
    let current = true;
    void isMicrosoftLoginAvailable().then((available) => {
      if (current) {
        setLoginAvailable(available);
      }
    });
    return () => {
      current = false;
    };
  }, []);

  const confirmRemove = async () => {
    if (!removing) {
      return;
    }
    try {
      if (removing.kind === "microsoft") {
        await removeMicrosoftAccount(removing.id);
      }
      remove(removing.id);
      setRemoving(null);
      setRemoveError(null);
    } catch (failure: unknown) {
      setRemoveError(describeCommandError(failure));
    }
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
            body="添加一个离线账户，或登录微软账户，就可以启动游戏了。离线账户只在本机使用，适合单机和局域网。"
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
                  <AccountAvatar
                    name={account.name}
                    uuid={account.kind === "microsoft" ? account.uuid : null}
                    size={48}
                  />
                  <div className="account-card-body">
                    <div className="account-card-title">
                      <Text weight="semibold" size={400} truncate wrap={false}>
                        {account.name}
                      </Text>
                      <Badge
                        appearance={account.kind === "microsoft" ? "tint" : "outline"}
                        color={account.kind === "microsoft" ? "success" : undefined}
                        size="small"
                      >
                        {account.kind === "microsoft" ? "正版" : "离线"}
                      </Badge>
                      {isActive ? (
                        <Badge appearance="tint" color="brand" size="small">
                          当前使用
                        </Badge>
                      ) : null}
                    </div>
                    <Text size={200} className="muted">
                      {account.kind === "microsoft"
                        ? "微软账户"
                        : `UUID：${describeUuidMode(account.uuidMode).label}`}
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
                        onClick={() => {
                          setRemoveError(null);
                          setRemoving(account);
                        }}
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
            <Button
              appearance="secondary"
              onClick={() => setLoginOpen(true)}
              disabled={loginAvailable !== true}
            >
              登录微软账户
            </Button>
          </div>
          <div className="inline-note" role="status">
            <ShieldLock24Regular aria-hidden="true" />
            <Text className="muted">
              {loginAvailable === null
                ? "正在检查微软登录是否可用…"
                : loginAvailable
                  ? "用浏览器完成登录，登录后账户会出现在上面的列表里。"
                  : isTauri()
                    ? "这个版本还没有配置微软登录（缺少 Azure 应用 ID），暂时只能使用离线账户。"
                    : "微软登录需要桌面版，浏览器预览里无法完成授权。"}
            </Text>
          </div>
        </div>
      </section>

      <AddOfflineAccountDialog open={adding} onOpenChange={setAdding} />
      <MicrosoftLoginDialog
        open={loginOpen}
        onOpenChange={setLoginOpen}
        onSignedIn={(account) =>
          signInMicrosoft({
            id: account.id,
            kind: "microsoft",
            name: account.displayName,
            uuid: account.uuid,
          })
        }
      />

      <Dialog open={removing !== null} onOpenChange={(_, data) => !data.open && setRemoving(null)}>
        <DialogSurface aria-describedby={undefined}>
          <DialogBody>
            <DialogTitle>移除账户「{removing?.name}」？</DialogTitle>
            <DialogContent>
              {removing?.kind === "microsoft" ? (
                <Text>
                  会从这台电脑上退出这个微软账户，并删除系统凭据库里保存的登录。
                  之后可以随时重新登录，存档不受影响。
                </Text>
              ) : removing?.uuidMode === "standard" ? (
                <Text>之后可以用同样的用户名重新添加，UUID 不会变，存档不受影响。</Text>
              ) : (
                <Text>
                  这个账户的 UUID 只保存在这里，移除后无法找回。如果存档依赖它，请先记下 UUID：
                  <span className="mono"> {removing?.uuid ? formatUuid(removing.uuid) : ""}</span>
                </Text>
              )}
              {removeError ? (
                <Text role="alert" className="muted">
                  {removeError}
                </Text>
              ) : null}
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" onClick={() => setRemoving(null)}>
                取消
              </Button>
              <Button
                appearance="primary"
                onClick={() => void confirmRemove()}
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
