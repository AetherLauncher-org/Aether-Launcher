import { Link } from "@fluentui/react-components";
import { Article, H2, P, UL } from "../prose";
import { lastUpdated, repositoryUrl } from "../router";
import type { Lang } from "../router";

const apache = "https://www.apache.org/licenses/LICENSE-2.0";
const minecraftEula = "https://www.minecraft.net/eula";

function English() {
  return (
    <Article title="Terms of use" meta={`Last updated ${lastUpdated}.`}>
      <H2>The short version</H2>
      <UL>
        <li>Aether Launcher is free, open-source software. You may use, copy and change it under the Apache-2.0 license.</li>
        <li>You need your own copy of Minecraft: Java Edition. Aether does not provide, sell or unlock the game.</li>
        <li>It comes without any warranty. Back up worlds you care about.</li>
      </UL>

      <H2>License</H2>
      <P>
        The software is licensed under the{" "}
        <Link href={apache} rel="noopener">
          Apache License 2.0
        </Link>
        . These terms do not replace that license or add restrictions to it; where they differ, the license applies to the
        software.
      </P>

      <H2>Minecraft and Microsoft</H2>
      <UL>
        <li>
          Aether is an unofficial project. It is not approved by or associated with Mojang Studios or Microsoft.
        </li>
        <li>
          The game, its files and your Microsoft account remain subject to the{" "}
          <Link href={minecraftEula} rel="noopener">
            Minecraft EULA
          </Link>{" "}
          and Microsoft’s terms. Aether downloads game files only from Mojang’s official services and signs you in only
          through Microsoft’s own sign-in.
        </li>
        <li>Use Aether only with a Minecraft account and game copy you are entitled to use.</li>
      </UL>

      <H2>Mods and other content</H2>
      <P>
        Mod loaders such as Forge, NeoForge and Fabric, and the mods you add, come from their own authors under their own
        terms. Aether verifies the files it downloads against the published checksums, but it cannot judge what a mod does.
        Install mods you trust.
      </P>

      <H2>No warranty</H2>
      <P>
        Aether is provided “as is”, without warranties of any kind, as stated in sections 7 and 8 of the Apache License.
        The authors are not liable for lost worlds, crashes or other damage that results from using it.
      </P>

      <H2>This website</H2>
      <P>
        The website is informational. It has no accounts and collects nothing; see the privacy page for details. The
        documentation may be out of date; if it disagrees with the source code, the source code is right.
      </P>

      <H2>Changes and questions</H2>
      <P>
        We may update these terms; the date above changes when we do, and the history is public in the repository. Open an
        issue on{" "}
        <Link href={`${repositoryUrl}/issues`} rel="noopener">
          GitHub
        </Link>{" "}
        with any question.
      </P>
    </Article>
  );
}

function Chinese() {
  return (
    <Article title="使用条款" meta={`最后更新 ${lastUpdated}。`}>
      <H2>简短版本</H2>
      <UL>
        <li>Aether Launcher 是免费的开源软件，你可以按 Apache-2.0 许可证使用、复制和修改它。</li>
        <li>你需要自己的 Minecraft Java 版。Aether 不提供、不出售，也不会解锁游戏。</li>
        <li>软件不附带任何担保。重要的存档请自己备份。</li>
      </UL>

      <H2>许可证</H2>
      <P>
        软件以{" "}
        <Link href={apache} rel="noopener">
          Apache License 2.0
        </Link>{" "}
        许可。本条款不取代这份许可证，也不在它之上增加限制；两者不一致时，软件以许可证为准。
      </P>

      <H2>Minecraft 与微软</H2>
      <UL>
        <li>Aether 是非官方项目，未获得 Mojang Studios 或微软的认可，也与它们没有关联。</li>
        <li>
          游戏本体、游戏文件和你的微软账户仍受{" "}
          <Link href={minecraftEula} rel="noopener">
            Minecraft 最终用户许可协议
          </Link>{" "}
          和微软条款约束。Aether 只从 Mojang 官方服务下载游戏文件，只通过微软自己的登录页面让你登录。
        </li>
        <li>请只在你有权使用的 Minecraft 账户和游戏副本上使用 Aether。</li>
      </UL>

      <H2>模组和其他内容</H2>
      <P>
        Forge、NeoForge、Fabric 等模组加载器以及你添加的模组，来自各自的作者，适用各自的条款。Aether
        会按官方公布的校验值检查它下载的文件，但判断不了一个模组会做什么。请只安装你信任的模组。
      </P>

      <H2>不提供担保</H2>
      <P>
        Aether 按「现状」提供，不附带任何形式的担保，具体见 Apache 许可证第 7、8
        条。因使用它而丢失存档、崩溃或造成其他损失，作者不承担责任。
      </P>

      <H2>这个网站</H2>
      <P>
        网站只用于介绍，没有账户，也不收集任何数据，详见隐私页。文档可能落后于代码；如果和源码不一致，以源码为准。
      </P>

      <H2>变更与提问</H2>
      <P>
        我们可能会更新这些条款，更新时上面的日期会变，历史记录在仓库里公开可查。有问题请到{" "}
        <Link href={`${repositoryUrl}/issues`} rel="noopener">
          GitHub
        </Link>{" "}
        提 issue。
      </P>
    </Article>
  );
}

export function Terms({ lang }: { lang: Lang }) {
  return lang === "en" ? <English /> : <Chinese />;
}
