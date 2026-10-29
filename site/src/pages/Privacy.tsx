import {
  Link,
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableHeaderCell,
  TableRow,
  makeStyles,
} from "@fluentui/react-components";
import { Article, Code, H2, P, UL } from "../prose";
import { lastUpdated, repositoryUrl } from "../router";
import type { Lang } from "../router";

const useStyles = makeStyles({
  tableWrap: { overflowX: "auto", marginBlock: "12px" },
  cell: { alignItems: "flex-start", paddingBlock: "8px" },
  hosts: { display: "grid", rowGap: "4px" },
});

const consent = "https://account.live.com/consent/Manage";
const cloudflare = "https://www.cloudflare.com/privacypolicy/";

type Row = { when: string; hosts: string[]; why: string };

function Network({ head, rows }: { head: [string, string, string]; rows: Row[] }) {
  const styles = useStyles();
  return (
    <div className={styles.tableWrap}>
      <Table aria-label={head[1]} size="small">
        <TableHeader>
          <TableRow>
            {head.map((label) => (
              <TableHeaderCell key={label}>{label}</TableHeaderCell>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow key={row.when}>
              <TableCell className={styles.cell}>{row.when}</TableCell>
              <TableCell className={styles.cell}>
                <span className={styles.hosts}>
                  {row.hosts.map((host) => (
                    <Code key={host}>{host}</Code>
                  ))}
                </span>
              </TableCell>
              <TableCell className={styles.cell}>{row.why}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

const mojangHosts = [
  "piston-meta.mojang.com",
  "piston-data.mojang.com",
  "libraries.minecraft.net",
  "resources.download.minecraft.net",
];
const signInHosts = [
  "login.microsoftonline.com",
  "user.auth.xboxlive.com",
  "xsts.auth.xboxlive.com",
  "api.minecraftservices.com",
];

function English() {
  return (
    <Article title="Privacy" meta={`Last updated ${lastUpdated}.`}>
      <H2>The short version</H2>
      <UL>
        <li>Aether has no accounts, no servers of its own that receive your data, and no telemetry or analytics.</li>
        <li>
          Your Microsoft sign-in is handled directly between your computer and Microsoft, Xbox Live and Minecraft
          services. Aether’s website never sees it.
        </li>
        <li>Everything Aether remembers stays on your computer.</li>
      </UL>

      <H2>The launcher</H2>
      <P>Aether contacts only the services it needs to do what you ask:</P>
      <Network
        head={["When", "Service", "Why"]}
        rows={[
          { when: "Browsing and installing versions", hosts: mojangHosts, why: "Mojang’s official version list and game files." },
          {
            when: "Signing in with Microsoft",
            hosts: signInHosts,
            why: "The device code sign-in, Xbox Live and Minecraft services, to check that you own the game and read your player name and UUID.",
          },
        ]}
      />

      <H2>Microsoft sign-in</H2>
      <UL>
        <li>
          Aether asks Microsoft for the <Code>XboxLive.signin</Code> and <Code>offline_access</Code> permissions, nothing
          else. It cannot read your mail, files or contacts.
        </li>
        <li>
          It stores your player name, Minecraft profile UUID and Xbox user ID in a small file on your computer, and the
          refresh token in the operating system’s credential vault.
        </li>
        <li>
          “Remove account” deletes both. You can also revoke Aether’s access at any time at{" "}
          <Link href={consent} rel="noopener">
            account.live.com/consent/Manage
          </Link>
          .
        </li>
      </UL>

      <H2>On your computer</H2>
      <P>
        Instances, settings and the account list are stored in the application’s data folder. Crash and game logs stay
        local; Aether never uploads them. If you share a log with someone, check it first, although Aether removes known
        credential patterns when it reads logs.
      </P>

      <H2>This website</H2>
      <P>
        This site is a small static page app served from this domain. It sets no cookies, loads no third-party scripts and
        includes no analytics or trackers. It is hosted on Cloudflare Pages, which, like any host, may keep ordinary
        server logs as described in{" "}
        <Link href={cloudflare} rel="noopener">
          Cloudflare’s privacy policy
        </Link>
        .
      </P>

      <H2>Questions</H2>
      <P>
        Open an issue on{" "}
        <Link href={`${repositoryUrl}/issues`} rel="noopener">
          GitHub
        </Link>
        . If this page and the source ever disagree, the source is right, and the page is a bug we want to hear about.
      </P>
    </Article>
  );
}

function Chinese() {
  return (
    <Article title="隐私" meta={`最后更新 ${lastUpdated}。`}>
      <H2>简短版本</H2>
      <UL>
        <li>Aether 没有账户体系，没有接收你数据的自有服务器，也没有遥测或统计。</li>
        <li>你的微软登录只发生在你的电脑和微软、Xbox Live、Minecraft 服务之间，Aether 的网站看不到。</li>
        <li>Aether 记住的一切都留在你的电脑上。</li>
      </UL>

      <H2>启动器</H2>
      <P>Aether 只会连接完成你要求的事所必需的服务：</P>
      <Network
        head={["什么时候", "服务", "用途"]}
        rows={[
          { when: "浏览和安装版本", hosts: mojangHosts, why: "Mojang 官方的版本列表和游戏文件。" },
          {
            when: "用微软账户登录",
            hosts: signInHosts,
            why: "设备码登录、Xbox Live 和 Minecraft 服务，用来确认你拥有游戏，并读取玩家名和 UUID。",
          },
        ]}
      />

      <H2>微软登录</H2>
      <UL>
        <li>
          Aether 只向微软申请 <Code>XboxLive.signin</Code> 和 <Code>offline_access</Code>{" "}
          权限，没有别的。它读不到你的邮件、文件或联系人。
        </li>
        <li>玩家名、Minecraft 档案 UUID 和 Xbox 用户 ID 保存在你电脑上的一个小文件里，刷新令牌保存在操作系统的凭据库里。</li>
        <li>
          「移除账户」会把这两处都删掉。你也可以随时在{" "}
          <Link href={consent} rel="noopener">
            account.live.com/consent/Manage
          </Link>{" "}
          撤销 Aether 的授权。
        </li>
      </UL>

      <H2>在你的电脑上</H2>
      <P>
        实例、设置和账户列表保存在应用的数据目录里。崩溃日志和游戏日志只留在本地，Aether
        不会上传。如果你要把日志发给别人，请先自己检查一遍；Aether 读取日志时会去掉已知格式的凭据。
      </P>

      <H2>这个网站</H2>
      <P>
        本站是一个小型的静态页面应用，由这个域名提供：不设置 Cookie，不加载第三方脚本，也不含统计或跟踪。它托管在 Cloudflare
        Pages，和任何托管服务一样，可能会按{" "}
        <Link href={cloudflare} rel="noopener">
          Cloudflare 的隐私政策
        </Link>
        保留普通的服务器日志。
      </P>

      <H2>有问题？</H2>
      <P>
        到{" "}
        <Link href={`${repositoryUrl}/issues`} rel="noopener">
          GitHub
        </Link>{" "}
        提 issue。如果这个页面和源码不一致，以源码为准，这个页面就是需要修的错误。
      </P>
    </Article>
  );
}

export function Privacy({ lang }: { lang: Lang }) {
  return lang === "en" ? <English /> : <Chinese />;
}
