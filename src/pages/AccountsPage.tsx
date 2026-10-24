import {
  Badge,
  Button,
  Field,
  Input,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add24Regular,
  Delete24Regular,
  Person24Regular,
  ShieldLock24Regular,
} from "@fluentui/react-icons";
import { useState } from "react";
import type { FormEvent } from "react";
import { PageHeader } from "../components/PageHeader";
import { describeLoginStatus, startMicrosoftLogin } from "../ipc";
import type { MicrosoftLoginStatus } from "../ipc";
import {
  addOfflineAccount,
  maxPlayerNameLength,
  minPlayerNameLength,
  removeOfflineAccount,
  summariseOfflineAccounts,
} from "../lib/accounts";
import type { OfflineAccount } from "../lib/accounts";
import type { PageDefinition } from "../navigation";

type AccountsPageProps = {
  page: PageDefinition;
};

/**
 * 账户页。
 *
 * 微软账户：按钮会问 `startMicrosoftLogin()` 现在能不能登录，并把结果
 * 作为一条说明显示出来——不会出现「点了没反应」。
 * 离线账户：纯前端状态，可以真的添加和删除，刷新页面后不保留。
 *
 * 界面上不显示任何令牌。微软登录流程完成后，这里也只应该显示用户名与头像。
 */
export function AccountsPage({ page }: AccountsPageProps) {
  const [accounts, setAccounts] = useState<OfflineAccount[]>([]);
  const [draft, setDraft] = useState("");
  const [draftError, setDraftError] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<MicrosoftLoginStatus | null>(
    null,
  );
  const [loginPending, setLoginPending] = useState(false);

  const handleAdd = (event: FormEvent) => {
    event.preventDefault();
    const result = addOfflineAccount(accounts, draft);
    if (!result.ok) {
      setDraftError(result.error);
      return;
    }
    setAccounts(result.accounts);
    setDraft("");
    setDraftError(null);
  };

  const handleMicrosoft = async () => {
    setLoginPending(true);
    const status = await startMicrosoftLogin();
    setLoginStatus(status);
    setLoginPending(false);
  };

  return (
    <>
      <PageHeader title={page.label} description={page.summary} />

      <section className="account-grid" aria-label="账户">
        <article className="status-panel account-card">
          <div className="panel-heading">
            <Text as="h2" size={400} weight="semibold">
              微软账户
            </Text>
            <Badge appearance="tint" color="informative">
              正版
            </Badge>
          </div>
          <Text className="muted">
            使用微软账户登录，可以在正版服务器上验证身份。令牌只保存在本机，
            不会显示在界面上。
          </Text>

          <div className="account-actions">
            <Button
              appearance="primary"
              onClick={() => void handleMicrosoft()}
              disabled={loginPending}
            >
              {loginPending ? "请稍候…" : "使用微软账户登录"}
            </Button>
          </div>

          {loginStatus ? (
            <div className="inline-note" role="status">
              <ShieldLock24Regular aria-hidden="true" />
              <Text className="muted">{describeLoginStatus(loginStatus)}</Text>
            </div>
          ) : (
            <div className="inline-note">
              <ShieldLock24Regular aria-hidden="true" />
              <Text className="muted">
                还没有登录。登录流程完成后这里会显示账户名。
              </Text>
            </div>
          )}
        </article>

        <article className="status-panel account-card">
          <div className="panel-heading">
            <Text as="h2" size={400} weight="semibold">
              离线账户
            </Text>
            <Badge appearance="outline">
              {accounts.length === 0
                ? "未添加"
                : summariseOfflineAccounts(accounts)}
            </Badge>
          </div>
          <Text className="muted">
            离线账户只在本地使用，适合单机或局域网。玩家名 {minPlayerNameLength}–
            {maxPlayerNameLength} 位，只能使用字母、数字和下划线。
          </Text>

          <form className="offline-form" onSubmit={handleAdd}>
            <Field
              label="玩家名"
              validationMessage={draftError ?? undefined}
              validationState={draftError ? "error" : "none"}
            >
              <Input
                value={draft}
                placeholder="例如 Steve"
                maxLength={maxPlayerNameLength + 4}
                onChange={(_, data) => {
                  setDraft(data.value);
                  if (draftError) {
                    setDraftError(null);
                  }
                }}
              />
            </Field>
            <Button appearance="secondary" type="submit" icon={<Add24Regular />}>
              添加
            </Button>
          </form>

          {accounts.length === 0 ? (
            <div className="empty-state compact">
              <Person24Regular aria-hidden="true" />
              <Text weight="semibold">还没有离线账户</Text>
              <Text className="muted">填入玩家名即可添加一个本地账户。</Text>
            </div>
          ) : (            <ul className="account-list">
              {accounts.map((account) => (
                <li key={account.id}>
                  <Person24Regular aria-hidden="true" />
                  <Text className="account-name">{account.name}</Text>
                  <Badge appearance="outline">离线</Badge>
                  <Tooltip content="移除此账户" relationship="label">
                    <Button
                      appearance="subtle"
                      icon={<Delete24Regular />}
                      aria-label={`移除 ${account.name}`}
                      onClick={() =>
                        setAccounts((previous) =>
                          removeOfflineAccount(previous, account.id),
                        )
                      }
                    />
                  </Tooltip>
                </li>
              ))}
            </ul>
          )}
        </article>
      </section>

      <Text className="page-footnote muted">
        离线账户只保存在本次会话里，重启启动器后需要重新添加。
      </Text>
    </>
  );
}
