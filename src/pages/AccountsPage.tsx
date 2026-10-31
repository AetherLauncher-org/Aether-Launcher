import {
  Badge,
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Tab,
  TabList,
  Text,
  Tooltip,
} from "@fluentui/react-components";
import {
  Add20Regular,
  Delete20Regular,
  Globe20Regular,
  ShieldCheckmark20Regular,
  ShieldLock24Regular,
} from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { AccountAvatar } from "../components/AccountAvatar";
import { AddOfflineAccountDialog } from "../components/AddOfflineAccountDialog";
import { MicrosoftLoginDialog } from "../components/MicrosoftLoginDialog";
import { PageHeader } from "../components/PageHeader";
import { SkinLibrary } from "../components/SkinLibrary";
import {
  describeCommandError,
  isMicrosoftLoginAvailable,
  isTauri,
  removeMicrosoftAccount,
} from "../ipc";
import { describeUuidMode, formatUuid } from "../lib/accounts";
import type { Account } from "../lib/accounts";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";

type AccountsPageProps = {
  page: PageDefinition;
};

/** 账户分三类，列表和页头的按钮都按这个顺序：微软、第三方、离线。 */
type Group = "microsoft" | "thirdparty" | "offline";

const groups: readonly {
  id: Group;
  title: string;
  /** 这一类下面一个账户也没有时显示的一句话 */
  empty: string;
}[] = [
  {
    id: "microsoft",
    title: "微软账户",
    empty: "还没有微软账户。点上面的「登录微软账户」用浏览器登录；令牌只保存在本机的系统凭据库里，不会显示在界面上。",
  },
  {
    id: "thirdparty",
    title: "第三方账户",
    empty: "还没有第三方账户。LittleSkin、Ely.by 这类外置登录服务器的登录还在做，暂时不能用。",
  },
  {
    id: "offline",
    title: "离线账户",
    empty: "还没有离线账户。离线账户只在本机使用，适合单机和局域网；点上面的「添加离线账户」。",
  },
];

/** 一个账户属于哪一类。第三方账户还没有，所以现在只有微软和离线两类有人。 */
function groupOf(account: Account): Group {
  return account.kind === "microsoft" ? "microsoft" : "offline";
}

/**
 * 账户页。点一张账户卡片就直接使用这个账户（卡片就是按钮，没有单独的「使用」按钮）。页头三个按钮从左到右：登录微软账户（主色）、第三方登录、添加离线账户；下面的列表按
 * 同样的顺序分三类。离线账户保存在本机并在重启后保留；微软账户走设备码或浏览器登录：令牌由后端保管，
 * 这里只显示账户身份。没有配置 Azure 应用 ID 的构建会如实说明，而不是点了没反应。第三方登录还没做，
 * 按钮在那里但点不了，悬停说明原因。
 */
export function AccountsPage({ page }: AccountsPageProps) {
  const { book, active, select, remove, signInMicrosoft } = useAccounts();
  const [tab, setTab] = useState<"accounts" | "skins">("accounts");
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

  const actions = (
    <div className="page-actions">
      <Button
        appearance="primary"
        icon={<ShieldCheckmark20Regular />}
        onClick={() => setLoginOpen(true)}
        disabled={loginAvailable !== true}
      >
        登录微软账户
      </Button>
      <Tooltip content="还没做：LittleSkin、Ely.by 等外置登录服务器" relationship="description">
        <Button appearance="secondary" icon={<Globe20Regular />} disabledFocusable>
          第三方登录
        </Button>
      </Tooltip>
      <Button appearance="secondary" icon={<Add20Regular />} onClick={() => setAdding(true)}>
        添加离线账户
      </Button>
    </div>
  );

  return (
    <>
      <PageHeader title={page.label} description={page.summary} action={tab === "accounts" ? actions : undefined} />

      <TabList
        selectedValue={tab}
        onTabSelect={(_, data) => setTab(data.value as "accounts" | "skins")}
        aria-label="账户和皮肤"
      >
        <Tab value="accounts">账户</Tab>
        <Tab value="skins">皮肤库</Tab>
      </TabList>

      {tab === "skins" ? (
        <SkinLibrary />
      ) : (
        <>

      {loginAvailable === false ? (
        <div className="inline-note account-note" role="status">
          <ShieldLock24Regular aria-hidden="true" />
          <Text className="muted">
            {isTauri()
              ? "这个版本还没有配置微软登录（缺少 Azure 应用 ID），暂时只能使用离线账户。"
              : "微软登录需要桌面版，浏览器预览里无法完成授权。"}
          </Text>
        </div>
      ) : null}

      {groups.map((group) => {
        const accounts = book.accounts.filter((account) => groupOf(account) === group.id);
        return (
          <section
            key={group.id}
            aria-labelledby={`${group.id}-heading`}
            className="section"
            data-account-group={group.id}
          >
            <div className="section-heading">
              <Text id={`${group.id}-heading`} as="h2" size={500} weight="semibold">
                {group.title}
              </Text>
              {group.id === "microsoft" ? (
                <Badge appearance="tint" color="success">
                  正版
                </Badge>
              ) : null}
              <Text className="muted">{accounts.length} 个</Text>
            </div>

            {accounts.length === 0 ? (
              <Text className="muted account-group-empty">{group.empty}</Text>
            ) : (
              <ul className="account-grid">
                {accounts.map((account, index) => {
                  const isActive = active?.id === account.id;
                  return (
                    <li
                      key={account.id}
                      className={`account-card${isActive ? " active" : ""}`}
                      style={{ "--i": index } as CSSProperties}
                    >
                      <button
                        type="button"
                        className="account-card-main"
                        aria-label={`使用 ${account.name}`}
                        aria-pressed={isActive}
                        onClick={() => select(account.id)}
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
                      </button>
                      <div className="account-card-actions">
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
        );
      })}

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
      )}
    </>
  );
}
