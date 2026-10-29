import { Link } from "@fluentui/react-components";
import { Article, Callout, H2, P, UL } from "../prose";
import { lastUpdated, repositoryUrl } from "../router";
import type { Lang } from "../router";

const rules = `${repositoryUrl}/blob/main/AGENTS.md`;
const advisory = `${repositoryUrl}/security/advisories/new`;

function English() {
  return (
    <Article
      title="Security model"
      meta={
        <>
          Last updated {lastUpdated}. These rules are enforced in code and tests; the source of truth is{" "}
          <Link href={rules} rel="noopener">
            AGENTS.md
          </Link>{" "}
          in the repository.
        </>
      }
    >
      <H2>1. The interface can only ask for narrow things</H2>
      <P>
        The window you see is a web view. It can call only a short, fixed list of backend commands, each with typed
        arguments such as an instance ID or a version number. There is no command that runs an arbitrary program or reads
        or writes an arbitrary path, and the interface never sends file system paths: folders are picked by the backend,
        and the interface gets back an opaque, single-use token.
      </P>

      <H2>2. Credentials never leave the backend</H2>
      <UL>
        <li>
          Access tokens, refresh tokens and device codes are never serialized to the interface, written to logs or crash
          reports, or saved in plain files.
        </li>
        <li>
          The only long-lived secret is the Microsoft refresh token. It is stored in the operating system’s credential vault
          (Windows Credential Manager, macOS Keychain or Linux Secret Service). If the vault is unavailable, Aether does
          not fall back to a file.
        </li>
        <li>
          Microsoft, Xbox and Minecraft access tokens exist only in memory during sign-in or while the game is being
          started.
        </li>
        <li>
          Token types redact themselves when printed, and error messages carry only a status code or an error label, never
          a response body.
        </li>
      </UL>

      <H2>3. Downloads are verified against the official manifest</H2>
      <UL>
        <li>
          Every file’s destination must be a safe relative path inside Aether’s own game folder. Paths that escape it,
          absolute paths and links that point elsewhere are refused.
        </li>
        <li>
          Every file is checked by SHA-1 and size, and the expected hash always comes from Mojang’s official manifest. A
          mirror, or a file found in another launcher’s folder, is never trusted on its own: a copied file is used only if
          it matches the official hash.
        </li>
        <li>
          Downloads are written to a temporary name and renamed into place only after verification, so an interrupted
          install never leaves a half-valid file.
        </li>
      </UL>

      <H2>4. Manifests, logs and crash reports are untrusted input</H2>
      <P>
        Version manifests, other launchers’ configuration files, game logs and crash reports are all treated as hostile.
        They are parsed with size and depth limits, symbolic links are refused, and nothing in them is ever executed.
      </P>

      <H2>Your existing files</H2>
      <P>
        Importing from another launcher is read-only. Aether copies verified files out and never modifies, moves or
        deletes anything in the other launcher’s folder.
      </P>

      <Callout>
        Found a vulnerability? Please report it through{" "}
        <Link href={advisory} rel="noopener">
          a private GitHub security advisory
        </Link>{" "}
        rather than a public issue.
      </Callout>
    </Article>
  );
}

function Chinese() {
  return (
    <Article
      title="安全模型"
      meta={
        <>
          最后更新 {lastUpdated}。这些规则由代码和测试保证，以仓库里的{" "}
          <Link href={rules} rel="noopener">
            AGENTS.md
          </Link>{" "}
          为准。
        </>
      }
    >
      <H2>1. 界面只能提出很窄的请求</H2>
      <P>
        你看到的窗口是一个网页视图。它只能调用一份固定的、很短的后端命令清单，参数是有类型的，比如实例 ID
        或版本号。没有任何命令可以运行任意程序，或读写任意路径；界面也从不发送文件系统路径：文件夹由后端来选，界面拿到的只是一个一次性的不透明令牌。
      </P>

      <H2>2. 凭据不会离开后端</H2>
      <UL>
        <li>访问令牌、刷新令牌和设备码不会被序列化给界面、写进日志或崩溃报告，也不会存成明文文件。</li>
        <li>
          唯一长期保存的秘密是微软刷新令牌，保存在操作系统的凭据库里（Windows 凭据管理器、macOS 钥匙串或 Linux Secret
          Service）。凭据库不可用时，Aether 不会退回到写文件。
        </li>
        <li>微软、Xbox 和 Minecraft 的访问令牌只在登录期间或启动游戏的过程中存在于内存里。</li>
        <li>令牌类型被打印时会自动打码，错误信息只带状态码或错误标签，从不带响应内容。</li>
      </UL>

      <H2>3. 下载按官方清单校验</H2>
      <UL>
        <li>每个文件的目标都必须是 Aether 自己游戏目录里的安全相对路径。逃出目录的路径、绝对路径和指向别处的链接都会被拒绝。</li>
        <li>
          每个文件都校验 SHA-1
          和大小，期望的哈希始终来自 Mojang 官方清单。镜像、或者在其他启动器文件夹里找到的文件，都不会被单独信任：复制来的文件只有和官方哈希一致才会使用。
        </li>
        <li>下载先写到临时文件名，校验通过后才改名到位，所以中断的安装不会留下一个看似有效的残缺文件。</li>
      </UL>

      <H2>4. 清单、日志和崩溃报告都是不可信输入</H2>
      <P>
        版本清单、其他启动器的配置文件、游戏日志和崩溃报告都按恶意输入处理：解析时有大小和深度上限，拒绝符号链接，里面的内容从不被执行。
      </P>

      <H2>你已有的文件</H2>
      <P>从其他启动器导入是只读的。Aether 只把校验通过的文件复制出来，不会修改、移动或删除对方文件夹里的任何东西。</P>

      <Callout>
        发现了安全漏洞？请通过{" "}
        <Link href={advisory} rel="noopener">
          GitHub 的私密安全公告
        </Link>
        报告，不要开公开的 issue。
      </Callout>
    </Article>
  );
}

export function Security({ lang }: { lang: Lang }) {
  return lang === "en" ? <English /> : <Chinese />;
}
