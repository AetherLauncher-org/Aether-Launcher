# 第三方账户登录（Yggdrasil / authlib-injector）

用外置登录服务器（LittleSkin、Ely.by 以及任何实现 authlib-injector Yggdrasil 协议的服务）登录。这篇讲协议怎么走、令牌放在哪、服务器地址怎么校验，以及**哪些还没做**（agent 下载和启动参数）。

## 现状

代码分三层，职责和 [架构概览](../architecture.md) 一致：

- `aether-auth::yggdrasil`：纯协议的 `build_*` / `parse_*`，离线可测，不联网、不落盘。
- `aether-app::yggdrasil`：账户服务，持有「登录中的会话」和「已保存的账户」，走 `Host` trait 拿数据目录。
- `src-tauri/src/commands/yggdrasil.rs`：只做参数转换的薄命令。

账户登录、刷新、移除都已经完成。**用第三方账户启动游戏还没做**，见下面「还没做」。

## 协议怎么走

外置登录服务器在它的 API 根目录下提供 `authserver/` 四个端点，和 Mojang 老版 Yggdrasil 兼容：

| 端点 | 作用 | 请求体要点 |
| --- | --- | --- |
| `authenticate` | 用户名 + 密码换令牌 | `agent`（`Minecraft`/`1`）、`username`、`password`、`clientToken`（启动器随机生成的 UUID）、`requestUser:false` |
| `refresh` | 用访问令牌换新的访问令牌；可带 `selectedProfile` 绑定角色 | `accessToken`、`clientToken`，可选 `selectedProfile` |
| `validate` | 校验访问令牌是否仍有效 | `accessToken`、`clientToken` |
| `invalidate` | 作废访问令牌 | `accessToken`、`clientToken` |

API 根地址（返回 `skinDomains` 和 `signaturePublickey`）本身也是一个 `GET`，登录时先读它确认服务是 authlib-injector 服务器。

登录流程：

1. 前端调用 `begin_yggdrasil_login`，传服务器地址、用户名、密码。密码只用来构造这一次 `authenticate` 请求体，**不进入任何状态、存储、日志或返回**。
2. 后端校验服务器地址（见下），读 API 元数据，然后 `authenticate`。响应里的 `selectedProfile`（若有）或 `availableProfiles` 就是可选角色。
3. 有多个角色时后端返回角色列表，前端弹「选择角色」，再调用 `finish_yggdrasil_login` 传选中的 `profileId`。只有一个角色时前端直接用它。
4. 选中角色后，如果 `authenticate` 没直接返回 `selectedProfile`，后端用 `refresh` 带 `selectedProfile` 绑定；绑定成功后把 `accessToken` + `clientToken` 打包存进系统凭据库。
5. 后端返回账户身份（ID、服务器、角色），前端列进「第三方账户」。

角色选择期间后端用内存里的 `GATE` 互斥锁串行化，会话 5 分钟过期；绑定后的令牌先写回会话，凭据库写入失败时可以只重试保存而不再刷新一次。

## 令牌放在哪

| 内容 | 位置 | 说明 |
| --- | --- | --- |
| 访问令牌 + client token | 系统凭据库（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service） | 打包成一个带 `#END` 后缀的字符串存进去；`Debug`/`Display` 都打码 |
| 账户信息（角色 UUID、名字、服务器根地址） | `app_data/yggdrasil-accounts/<uuid>.json` | 只有元数据，文件格式里没有令牌字段（和微软账户同一套 `FileCredentialStore`，有大小上限、拒绝明文令牌字段、拒绝符号链接） |
| 密码 | 不保存 | 只存在于一次 `begin_yggdrasil_login` 调用的参数里 |

- **令牌永远不经过 IPC。** 前端只看到账户身份（ID、服务器、角色 UUID 和名字）。密码只进 `begin_yggdrasil_login` 的入参，不出。
- 刷新（`refresh_yggdrasil_account`）会用 `refresh` 端点轮换令牌并写回凭据库；`refresh` 响应的 `selectedProfile` 必须和已保存角色的 ID 一致，否则拒绝。
- 移除（`remove_yggdrasil_account`）**尽力而为**地先 `invalidate` 再删除本地凭据：服务器离线时也能退出，不会因为网络失败删不掉本地账户。
- 重新登录同一服务器同一角色会更新原账户（不重复建一条）。
- 所有错误都是稳定的短字符串（`yggdrasil-*`），不含响应体、令牌或完整 URL；前端翻译成中文，不认识的兜底为一句通用提示。

## 服务器地址怎么校验

`Server::parse` 只接受满足全部条件的地址，否则 `yggdrasil-invalid-server`：

- `https://` 开头，长度 ≤ 2048；
- 主机名是正常的域名（至少两段、每段 1–63 字符、只含字母数字和 `-`、不以 `-` 开头结尾、最后一段全字母），拒绝 IP 字面量、`localhost`、`.local`/`.internal`/`.lan` 等；
- 没有 `user:pass@`（无 userinfo）、没有查询串或片段、没有 `:443` 之外的端口、没有路径穿越（`.`/`..`）；
- 归一化成小写、以 `/` 结尾的规范根地址。

前端只是顺手预校验（同样的规则），真正的校验和 DNS 解析都在后端。

**内网策略**：`aether-app::yggdrasil_transport` 的 DNS 解析器在连接前检查解析出的每个 socket 地址，只允许公网地址——拒绝回环、私网（RFC 1918）、链路本地、组播、保留段和 `100.64.0.0/10` 等过渡地址，IPv6 只放行全局单播。这样「看起来像公网域名、实际解析到内网」的服务器不能用来探测用户内网。不跟随重定向、不走代理（`proxy(None)`），响应体有 256 KiB 上限。

## 前端的 IPC

| 命令 | 作用 |
| --- | --- |
| `begin_yggdrasil_login {server, username, password}` | 开始登录，返回 `{id, profiles}`；同一时间只允许一个 |
| `finish_yggdrasil_login {id, profileId}` | 绑定角色并保存账户，返回账户身份 |
| `cancel_yggdrasil_login {id}` | 放弃登录（尽力作废已拿到的令牌） |
| `list_yggdrasil_accounts` | 已登录的第三方账户（只读元数据） |
| `refresh_yggdrasil_account {id}` | 刷新并轮换令牌 |
| `remove_yggdrasil_account {id}` | 退出账户 |

界面里的第三方账户**和启动用的账户簿分开**：现在不能拿第三方账户启动游戏，卡片上标了「暂不能用于启动」。

## 还没做（启动接线，明确留白）

登录、刷新、移除都已完成；**用第三方账户启动游戏**留作后续项，原因是它牵涉两个尚未解决的部分：

1. **authlib-injector agent JAR 的下载与校验**。启动要用 `-javaagent:authlib-injector.jar=<API 根地址>`，agent JAR 必须来自官方发布。官方发布地址和 SHA-256 清单**尚未核实**——AGENTS.md 规定「哈希必须来自官方清单，镜像不是信任根」，在没有拿到官方清单里的 SHA-256 之前不写死任何哈希或镜像地址，也不实现下载。要做时：从官方发布取 JAR 和它的校验和清单，校验 SHA-256，存到受管理的相对路径，再拼 JVM 参数。
2. **启动参数里的 agent 参数**。`aether-launch` 现在按实例 ID 组装启动计划，第三方账户要额外注入 `-javaagent` 和对应的 `--accessToken`/`--uuid`/`--username` 变量，且访问令牌只进游戏进程 argv、不进日志/IPC/持久化（和微软账户同一套规则，见 [启动与进程](../design/launch-and-process.md)）。

这两个做完之前，第三方账户只做「登录 + 刷新 + 管理」，不接进启动流程，避免界面承诺了「会启动」但后端没有。

## 参考

协议行为参考了 authlib-injector 的公开文档和 LittleSkin、Ely.by 的接入说明，**只参考行为，没有复制代码**（这些项目各自的许可证与 Apache-2.0 的兼容性未逐一核实，不引入其代码）。Ely.by 的 API 根是 `https://account.ely.by/api/authlib-injector`（`authserver.ely.by` 是旧版、不要和 `authserver/` 后缀叠加使用）；LittleSkin 是 `https://littleskin.cn/api/yggdrasil`。

## 待定

- 换肤（`uploadSkin` / `uploadTexture`）是否对这些服务器开放、怎么接，见 [下一批功能](../design/next-features.md) 第 5 条。
- agent JAR 的官方发布地址和 SHA-256 清单，以及下载校验的具体流程（上面「还没做」第 1 点）。
