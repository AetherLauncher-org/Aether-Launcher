# Minecraft 下载源清单（调研版）

> 面向 Aether Launcher 用户和开发者
>
> 本页是下载源调研，不是架构决策，也不代表当前版本已经接入这些源。核验日期：**2026-10-21（Asia/Singapore）**。网络服务会变，`200 OK` 只代表这次探测成功，不能当作 SLA。

## 先说结论

- **默认基线**：保留 Mojang、各模组加载器和内容平台的官方上游，作为来源真相和最终回退
- **中国大陆的 Minecraft 文件加速**：优先评估 **BMCLAPI**，它覆盖版本元数据、Assets、Maven 库、Fabric/Forge/NeoForge 等多个文件族
- **高校镜像站**：可以作为 BMCLAPI Maven/Forge/NeoForge 的组件级候选，但很多站点是部分同步，不要把它们伪装成完整的 Minecraft 下载源
- **Modrinth 和 CurseForge**：它们是内容平台/API/CDN，不是 Mojang 镜像；需要按各自 API、速率限制和授权规则接入
- **MCBBS 源**：BMCLAPI 作者的历史帖子明确记录过它，但本次 HTTPS/HTTP 探测均失败，暂不放进默认回退链
- **Quilt 的 BMCLAPI 路由**：文档标注过“由于上游 API 存在 bug，暂时不可用”，本次 `/quilt-meta` 也返回 404，先走 Quilt 官方

Aether 的下载器以后应按“文件族”选择 provider，而不是把所有 URL 做一次粗暴的字符串替换。当前 Mojang 清单已经使用 `piston-meta.mojang.com` 和 `piston-data.mojang.com`，BMCLAPI 文档里仍保留较早的 `launchermeta.mojang.com` / `launcher.mojang.com` 映射，这两个时代不能混为一谈。

## 来源分层

| 层级 | 含义 | 能否直接当完整下载源 |
| --- | --- | --- |
| 官方上游 | 文件或元数据的权威发布方 | 可以，应该始终保留 |
| BMCLAPI / OpenBMCLAPI | 面向 Minecraft 文件的镜像与节点分发体系 | 可以，但要遵守其协议并做完整性校验 |
| 高校 BMCLAPI 镜像 | 同步 BMCLAPI 目录的一部分或全部 | 只按实际覆盖范围使用 |
| Modrinth / CurseForge | 模组、整合包等内容平台及 CDN | 不是 Minecraft 本体镜像 |
| 通用 Maven 镜像 | 面向 Java 构建依赖的代理或缓存 | 不应假设能提供 Minecraft 运行时文件 |

## 1. BMCLAPI：主候选

### 基本入口

- 文档：[BMCLAPI API 文档](https://bmclapidoc.bangbang93.com/)
- 主入口：`https://bmclapi2.bangbang93.com`
- 开源客户端中的实现参考：[bangbang93/BMCL 的 Mirrors 目录](https://github.com/bangbang93/BMCL/tree/master/BMCLV2/Mirrors)
- 节点分发项目：[bangbang93/openbmclapi](https://github.com/bangbang93/openbmclapi)

BMCLAPI 官方文档将它描述为面向 Minecraft/Forge 文件的开放 API，文件通常来自源站，BMCLAPI 只负责分发。文档还要求使用 BMCLAPI 的程序在下载界面或其他可视区域标明来源，并禁止在 BMCLAPI 上二次封装其他协议。Aether 如果接入，下载界面需要明确显示“BMCLAPI”，不能只在日志里悄悄替换。

### 已核验的路径族

| 文件族 | 官方上游 | BMCLAPI 路径/替换方式 | 本次结果 |
| --- | --- | --- | --- |
| 版本清单 | `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` | `https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json` | 200，JSON 结构和版本 ID 列表与官方一致 |
| 版本 JSON | 清单中的 `piston-meta.mojang.com/v1/packages/{sha1}/{id}.json` | `https://bmclapi2.bangbang93.com/v1/packages/{sha1}/{id}.json` | 200，示例文件 SHA-1 与官方一致 |
| Assets 索引 | 版本 JSON 的 `assetIndex.url` | 同一 `/v1/packages/{sha1}/{id}.json` 路径 | 200，示例索引对象数量一致 |
| Assets 对象 | `https://resources.download.minecraft.net/{前两位}/{sha1}` | `https://bmclapi2.bangbang93.com/assets/{前两位}/{sha1}` | 200，示例对象字节和 SHA-1 一致 |
| Minecraft Libraries | `https://libraries.minecraft.net/{path}` | `https://bmclapi2.bangbang93.com/maven/{path}` | 文档明确列出，Forge/NeoForge 样本可取 |
| Java runtime 元数据 | Mojang `launchermeta` 的 Java runtime 产品 JSON | 文档列出对应 `/v1/products/java-runtime/...` 路径 | 需按版本重新探测，不要硬编码文档里的旧 hash |
| Fabric Meta | `https://meta.fabricmc.net` | `https://bmclapi2.bangbang93.com/fabric-meta` | Loader API 样本与官方语义相等 |
| Fabric Maven | `https://maven.fabricmc.net/{path}` | `https://bmclapi2.bangbang93.com/maven/{path}` | 可用 |
| Forge Maven | `https://files.minecraftforge.net/maven/{path}` | `https://bmclapi2.bangbang93.com/maven/{path}` | 可用 |
| NeoForge Maven | `https://maven.neoforged.net/releases/net/neoforged/{forge,neoforge}/{path}` | `https://bmclapi2.bangbang93.com/maven/net/neoforged/{forge,neoforge}/{path}` | 可用 |
| authlib-injector | `https://authlib-injector.yushi.moe/` | `https://bmclapi2.bangbang93.com/mirrors/authlib-injector/` | 官方项目文档明确给出该镜像入口 |
| Quilt Meta/Maven | `https://meta.quiltmc.org` / `https://maven.quiltmc.org/repository/release` | `/quilt-meta` / `/maven` | 文档和本次探测都提示暂不可用，走官方 |

BMCLAPI 的下载接口可能返回 302，再转到 OpenBMCLAPI 节点或高校镜像节点。客户端必须正常跟随 HTTPS 重定向并继续校验目标文件，不能把某一次看到的节点域名写死，也不能用 `-k`/关闭证书校验来“修好”下载。

### BMCLAPI 的协议边界

- 文件（BMCLAPI 自身源码除外）归源站点所有
- BMCLAPI 只承诺尽量保证完整性、有效性和实时性，不承诺 Aether 需要的长期 SLA
- 使用 BMCLAPI 的程序必须在可视区域标明来源
- 不要把 BMCLAPI 的文件重新包装成另一个带有不同协议含义的服务
- OpenBMCLAPI 是节点分发实现，**不是另一个必须单独配置的完整公共源**；Aether 只需要把 BMCLAPI 当作 provider，保留其重定向行为

## 2. Mojang 官方上游：真相源和最终回退

- 版本清单：<https://piston-meta.mojang.com/mc/game/version_manifest_v2.json>
- 版本 JSON / Assets 索引：清单返回的 `piston-meta.mojang.com/v1/packages/...`
- 客户端、服务端和日志文件：清单返回的 `piston-data.mojang.com/v1/objects/...`
- Java 库：<https://libraries.minecraft.net/>
- Assets 对象：<https://resources.download.minecraft.net/>

官方清单本身携带版本 JSON 的 SHA-1，版本 JSON 又携带客户端、服务端、库文件和 Assets 索引的大小与散列。下载计划必须把这些值作为期望值，provider 只能改变传输地址，不能改变内容身份。

一个容易踩坑的点：BMCLAPI 文档中的旧映射主要针对 `launchermeta.mojang.com`、`launcher.mojang.com` 和 `resources.download.minecraft.net`。当前清单大量使用 `piston-meta.mojang.com`、`piston-data.mojang.com`，所以实现时要按 JSON 字段和文件族做显式适配，不能用全局 `replace(host, mirror)`。

## 3. 高校镜像站：组件级候选，不是完整 provider

这些站点公开了 `/bmclapi/` 目录，本次均能打开目录或取到 Forge/NeoForge 样本：

| 站点 | 入口 | 适合范围 | 证据与限制 |
| --- | --- | --- | --- |
| 上海交通大学 SJTUG | <https://mirror.sjtu.edu.cn/bmclapi/> | Maven、Forge、NeoForge，Assets 可能是部分快照 | 目录页标注由 `rsync-sjtug` 同步；某些路径会跳到对象存储 |
| 南京大学开源镜像站 | <https://mirror.nju.edu.cn/bmclapi/> | Maven、Forge、NeoForge，Assets 覆盖不完整 | Forge 样本可取；不要假设所有 hash 都有 |
| 齐鲁工业大学开源软件镜像站 | <https://mirrors.qlu.edu.cn/bmclapi/> | Maven、Forge、NeoForge，Assets 覆盖不完整 | Forge 样本可取；站点自身的目录结构可能变化 |
| 中国科学技术大学镜像站 | <https://mirrors.ustc.edu.cn/bmclapi/> | Maven、Forge、NeoForge，Assets 覆盖不完整 | Forge/NeoForge 样本可取；不要当作全量 Assets 源 |

本次用 `forge-1.20.1-47.4.0-installer.jar` 做样本，四个站点均返回 200 和 6,056,587 字节；官方 SHA-1 为 `b73203963ea70aee0c38dc58de1f6d16a766605b`。同一批站点对随机 Assets 对象 `b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356` 返回 404，说明它们不能直接当作完整 Assets 回退。

建议 provider 模型把它们拆成：

- `bmclapi-sjtu-maven`
- `bmclapi-nju-maven`
- `bmclapi-qlu-maven`
- `bmclapi-ustc-maven`

每个 provider 只声明自己实际覆盖的路径族，探测失败就回到 BMCLAPI 主入口或官方源。

## 4. Mod Loader 官方源与 BMCLAPI 映射

### Fabric

- 官方 Meta API：[FabricMC/fabric-meta](https://github.com/FabricMC/fabric-meta)
- Meta 入口：<https://meta.fabricmc.net/v2/versions>
- Maven：<https://maven.fabricmc.net/>
- BMCLAPI Meta：`https://bmclapi2.bangbang93.com/fabric-meta`
- BMCLAPI Maven：`https://bmclapi2.bangbang93.com/maven`

Fabric Meta 项目说明它是给工具和启动器查询 Fabric 项目版本信息的 JSON API，并标注大约每 5 分钟更新一次。调用时要保存返回的 loader/intermediary 版本、库 URL 和散列，不要只取“最新版本”字符串。

### Quilt

- 官方 Meta：<https://meta.quiltmc.org/v3/versions>
- 官方实现说明：[QuiltMC/quilt-meta](https://github.com/QuiltMC/quilt-meta)
- 官方 Maven：<https://maven.quiltmc.org/repository/release/>
- BMCLAPI 候选：`https://bmclapi2.bangbang93.com/quilt-meta`、`https://bmclapi2.bangbang93.com/maven`

本次 `GET https://bmclapi2.bangbang93.com/quilt-meta/v3/versions/loader/1.20.1` 返回 `404 COMMON_NO_SUCH_OBJECT`。这不是对 Quilt 官方服务可用性的判断，只说明当前 BMCLAPI Quilt 路由不能作为默认源。

### Forge

- 官方文件站：<https://files.minecraftforge.net/>
- 官方 Maven：<https://maven.minecraftforge.net/>
- 官方 promotions：<https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json>
- BMCLAPI：`https://bmclapi2.bangbang93.com/maven`
- BMCLAPI Forge API 文档：<https://bmclapidoc.bangbang93.com/#api-Forge>

Forge 官方文件站会把 Maven 路径重定向到 Maven 仓库。BMCLAPI 文档中的 `/forge/download` 等接口也可能 302 到真实文件地址，下载器要按文件元数据做校验，不要因为拿到 302 就当成已完成。

### NeoForge

- 官方文档中的 Maven 示例：[Installing a NeoForge Server](https://docs.neoforged.net/user/docs/server/)
- 官方 Maven：<https://maven.neoforged.net/releases/>
- BMCLAPI：`https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/` 和 `/maven/net/neoforged/forge/`

## 5. 内容平台：不是镜像，但启动器会用到

### Modrinth

- 官方 API 文档：<https://docs.modrinth.com/api/>
- API：<https://api.modrinth.com/v2/>
- 文件/CDN URL：由 API 返回，常见域名为 `cdn.modrinth.com`

Modrinth 的 API 文档说明多数读取请求不需要 token，但必须提供能唯一识别应用的 `User-Agent`，并有按 IP 计算的速率限制（文档当前写的是 300 次/分钟）。项目、版本和文件都有稳定 ID 或 SHA-1/SHA-512 字段，Aether 应缓存 API 元数据并按文件散列校验。Modrinth CDN 是 Modrinth 的内容分发，不应被标成 Mojang 镜像。

### CurseForge

- 官方 REST API：<https://docs.curseforge.com/rest-api/>
- API：<https://api.curseforge.com/v1/>
- 官方文档中的下载 URL 接口：`GET /v1/mods/{modId}/files/{fileId}/download-url`

CurseForge API 使用 `x-api-key`，而且官方在 2026 年的公告中说明 direct CDN（`edge.forgecdn.net`）的 API key 校验会逐步强制。Aether 不能把 key 写进桌面客户端、仓库或 CI 日志；如果未来要支持 CurseForge，需要明确授权、密钥托管和用户可见的来源标识。没有合法 key 时，不要把 403 当作“换个镜像域名”就能绕过。

## 6. authlib-injector

- 官方下载/API 文档：[获取 authlib-injector](https://github.com/yushijinhun/authlib-injector/blob/879c6cbfce848f37e7da656d59a20cb93bd7102b/wiki/src/zh/%E8%8E%B7%E5%8F%96-authlib-injector.md)
- 官方入口：<https://authlib-injector.yushi.moe/>
- BMCLAPI 镜像：`https://bmclapi2.bangbang93.com/mirrors/authlib-injector/`

官方文档明确列出 BMCLAPI 镜像，并要求使用时遵守 BMCLAPI 协议。该 API 返回构建号和 SHA-256，下载计划应优先使用 API 返回的 checksum，而不是把 jar 当成无校验附件。

## 7. MCBBS 源：记录，但暂不启用

BMCLAPI 作者的历史说明帖记录过：`https://download.mcbbs.net` 曾是 BMCLAPI 的特别镜像，目录结构和 API 与 BMCLAPI 一致。

本次核验结果：

- `https://download.mcbbs.net/mc/game/version_manifest_v2.json`：TLS 握手失败
- `http://download.mcbbs.net/...`：空响应
- 因此不能把它写入 Aether 的默认回退链

如果以后有人提交重新启用 MCBBS 源的 PR，至少要重新核验：版本清单、版本 JSON、Assets 对象、Maven 文件、TLS 证书、重定向链、散列一致性，并在文档里更新核验日期。

## 8. 通用 Maven 镜像：只给开发构建参考

以下类型的服务可以帮助 Gradle/Maven 拉取普通 Java 依赖，但没有证据表明它们是完整 Minecraft 运行时镜像：

- [阿里云 Maven 使用指南](https://maven.aliyun.com/mvn/guide)
- [华为云开源镜像站](https://mirrors.huaweicloud.com/home)
- [腾讯云镜像站](https://mirrors.cloud.tencent.com/)
- [中科大开源软件镜像站](https://mirrors.ustc.edu.cn/)
- [南京大学开源镜像站](https://mirror.nju.edu.cn/)

这些站点的 Maven repository 路径和代理范围需要按各自文档配置。本次用 Forge installer 路径做探测时，常见的通用 repository 入口均返回 404，而 BMCLAPI、高校 BMCLAPI 目录和 Forge 官方 Maven 均能取到文件。因此不要把“这是 Maven 镜像”写成“能下载 Minecraft 全部文件”。开发构建和玩家运行下载是两条不同链路。

## 9. Aether 接入时的校验契约

这部分是实现约束草案，不是架构文档：

1. **provider 只改地址，不改身份**
   - 期望 SHA-1/SHA-256、文件大小和相对路径来自官方或内容平台元数据
   - 任何 provider 返回的字节都必须通过同一套校验
2. **按文件族选择源**
   - manifest / version JSON
   - `piston-data` 客户端、服务端、日志
   - Assets
   - Mojang Libraries
   - loader Meta
   - loader Maven/installer
   - Modrinth/CurseForge 内容
3. **严格处理重定向**
   - 只允许 HTTPS
   - 限制最大重定向次数
   - 保留最终 URL、状态码、Content-Type 和 provider 名称用于诊断
   - 不关闭证书验证，不接受用户输入的任意最终地址
4. **失败要可解释**
   - 记录 `provider`, `artifact`, `attempt`, `status`, `reason`
   - 429/5xx/超时走有限重试和退避
   - 校验失败直接丢弃临时文件，不能把坏文件缓存成“成功”
5. **缓存不可伪造成功**
   - `pending | ready | failed | stale` 比“失败就当大小为 0”安全
   - provider 状态和文件状态分开记录
6. **来源可见**
   - 使用 BMCLAPI 的下载页面显示 BMCLAPI 来源
   - 使用 Modrinth/CurseForge 文件时显示对应平台和项目页
   - 不把第三方内容写成 Aether 自己发布

## 10. 可复现探测命令

下面的命令只做只读探测，不下载完整游戏。Windows PowerShell 可直接运行：

```powershell
# 官方与 BMCLAPI 版本清单
curl.exe -L --fail --silent --show-error `
  https://piston-meta.mojang.com/mc/game/version_manifest_v2.json `
  -o "$env:TEMP\mojang-manifest.json"

curl.exe -L --fail --silent --show-error `
  https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json `
  -o "$env:TEMP\bmclapi-manifest.json"

# 检查 JSON 可解析和版本数量
@(
  "$env:TEMP\mojang-manifest.json",
  "$env:TEMP\bmclapi-manifest.json"
) | ForEach-Object {
  $json = Get-Content $_ -Raw | ConvertFrom-Json
  "{0}: versions={1}, release={2}" -f $_, $json.versions.Count, $json.latest.release
}

# Forge 样本及 SHA-1
curl.exe -L --fail --silent --show-error `
  https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.0/forge-1.20.1-47.4.0-installer.jar `
  -o "$env:TEMP\forge-official.jar"
Get-FileHash "$env:TEMP\forge-official.jar" -Algorithm SHA1

curl.exe -L --fail --silent --show-error `
  https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.20.1-47.4.0/forge-1.20.1-47.4.0-installer.jar `
  -o "$env:TEMP\forge-bmclapi.jar"
Get-FileHash "$env:TEMP\forge-bmclapi.jar" -Algorithm SHA1
```

不要在脚本里加 `-k`。如果客户端验证到某个重定向节点的证书错误，应该把它记录成 provider 故障并回退，而不是关闭 TLS 验证。

## 11. 来源索引

### 第一方/运营方

- [BMCLAPI 文档](https://bmclapidoc.bangbang93.com/)
- [BMCL 源码中的镜像实现](https://github.com/bangbang93/BMCL/tree/master/BMCLV2/Mirrors)
- [OpenBMCLAPI README](https://github.com/bangbang93/openbmclapi/blob/master/README.md)
- [Mojang 版本清单](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json)
- [Fabric Meta README](https://github.com/FabricMC/fabric-meta/blob/master/README.md)
- [Quilt Meta README](https://github.com/QuiltMC/quilt-meta/blob/v3/README.md)
- [Forge 官方文件站](https://files.minecraftforge.net/)
- [NeoForge 官方服务端文档](https://docs.neoforged.net/user/docs/server/)
- [Modrinth API 文档](https://docs.modrinth.com/api/)
- [CurseForge REST API 文档](https://docs.curseforge.com/rest-api/)
- [authlib-injector 下载 API 文档](https://github.com/yushijinhun/authlib-injector/blob/879c6cbfce848f37e7da656d59a20cb93bd7102b/wiki/src/zh/%E8%8E%B7%E5%8F%96-authlib-injector.md)

### 历史/待复核

- [BMCLAPI 作者关于 MCBBS 源的历史说明（存档）](https://archives.mcbbs.co/read.php?tid=951042)

---

**维护提示**：新增 provider 前，先补一手来源、覆盖路径、协议/授权、一次真实探测和失败边界。只找到一个“看起来能下”的 URL，不足以把它列为默认源。
