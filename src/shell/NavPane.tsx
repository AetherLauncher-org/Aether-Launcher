import { Text } from "@fluentui/react-components";
import { AccountAvatar } from "../components/AccountAvatar";
import { useT } from "../i18n/context";
import { pageLabelKeys } from "../i18n/pages";
import { mainPages, settingsPage } from "../navigation";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";
import { useNavigation } from "../state/navigation";

type NavPaneProps = {
  version: string;
};

/**
 * 导航栏：在标题栏下面一横排。左边是主要页面（主页、实例、下载），右边是当前账户、设置和版本号；
 * 当前账户（头像和名字）就是进账户页的入口，没有单独的「账户」按钮。选中项的
 * 下边有一条指示条，切换时从中间向两边展开（见 `shell.css`）。窗口窄了只留图标，名称
 * 放在 `title` 里，读屏用的是 `aria-label`。
 */
export function NavPane({ version }: NavPaneProps) {
  const { current, navigate } = useNavigation();
  const { active } = useAccounts();
  const { t } = useT();

  const renderItem = (page: PageDefinition) => {
    const selected = page.id === current;
    const label = t(pageLabelKeys[page.id]);
    return (
      <button
        key={page.id}
        type="button"
        className={`nav-item${selected ? " selected" : ""}`}
        aria-label={label}
        aria-current={selected ? "page" : undefined}
        title={label}
        onClick={() => navigate(page.id)}
      >
        <span className="nav-icon" aria-hidden="true">
          {selected ? page.activeIcon : page.icon}
        </span>
        <span className="nav-label">{label}</span>
      </button>
    );
  };

  const accountAria = active ? t("nav.account.aria", { name: active.name }) : t("nav.account.none");
  const accountTitle = active
    ? t("nav.account.title", { name: active.name })
    : t("nav.account.add");
  const accountName = active?.name ?? t("nav.account.add");
  const accountKind = active
    ? active.kind === "microsoft"
      ? t("nav.account.kind.microsoft")
      : t("nav.account.kind.offline")
    : t("nav.account.none");

  return (
    <nav className="navbar" aria-label={t("nav.primary")}>
      <div className="nav-group">{mainPages.map(renderItem)}</div>

      <div className="nav-footer">
        <button
          type="button"
          className={`nav-item nav-account${current === "accounts" ? " selected" : ""}`}
          aria-label={accountAria}
          aria-current={current === "accounts" ? "page" : undefined}
          title={accountTitle}
          onClick={() => navigate("accounts")}
        >
          <span className="nav-icon" aria-hidden="true">
            <AccountAvatar
              name={active?.name ?? null}
              uuid={active?.kind === "microsoft" ? active.uuid : null}
              size={20}
            />
          </span>
          <span className="nav-label nav-account-text">
            <Text size={200} weight="semibold" truncate wrap={false}>
              {accountName}
            </Text>
            <Text size={100} className="nav-account-kind">
              {accountKind}
            </Text>
          </span>
        </button>
        {renderItem(settingsPage)}
        <Text size={100} className="nav-version">
          {t("nav.version", { version })}
        </Text>
      </div>
    </nav>
  );
}
