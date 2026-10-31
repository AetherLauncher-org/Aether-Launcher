# 微软账户登录

用微软账户登录才能进入正版服务器。这篇讲登录怎么走、令牌放在哪、以及**维护者要做什么才能让它在发行版里真正可用**（注册 Azure 应用并通过 Mojang 审核）。

## 现状

代码已经完整：设备码登录、浏览器授权登录（授权码 + PKCE）、Xbox Live、Minecraft 服务、系统凭据库存储、刷新、用正版令牌启动游戏。**没有配置 Azure 应用 ID 的构建里，登录按钮是灰的**，并写明原因，不会点了没反应。要让它真正能登录，需要下面「申请流程」里的两步。

## 登录怎么走

有两种方式拿到微软令牌，用户在「设置 → 账户登录」里选，选择只存在本机（`localStorage`），**默认是设备码**。两种方式之后的 Xbox、Minecraft 步骤、账户保存和事件完全相同。

### 设备码（默认）

设备码流程（RFC 8628），全程不需要在启动器里嵌浏览器，也不需要回调地址：

1. 前端调用 `begin_microsoft_login`。后端向微软申请设备码，把**展示用**的短码、验证网址和有效期返回给前端，同时在后台线程里开始轮询。
2. 用户点「打开登录页面」（后端用系统默认浏览器打开微软返回的网址，前端不传任何 URL），输入短码，用拥有 Minecraft 的账户登录并同意。
3. 轮询拿到微软令牌后，后端依次换取：Xbox Live 令牌 → XSTS 令牌 → Minecraft 访问令牌，再查是否拥有 Java 版（entitlements）和玩家档案。
4. 账户保存：档案 UUID 就是启动器的账户 ID，所以重复登录同一个账户是覆盖而不是新增。
5. 后端推送 `login://status` 事件：`waiting`、`signed-in`（带账户身份）、`failed`（带稳定的失败分类）、`cancelled`。

### 浏览器授权（授权码 + PKCE）

登录后浏览器自动跳回启动器，不用抄码。要求应用注册里登记了回调地址（见下面「申请流程」第 1 步）。

1. 前端调用 `begin_microsoft_browser_login`，**什么都不传**。后端在 `127.0.0.1` 上绑一个空闲端口，生成 PKCE 的 verifier（64 位十六进制，244 位随机）、其 `S256` challenge 和随机 `state`，拼出微软的授权网址（`redirect_uri=http://localhost:<端口>`），用系统默认浏览器打开它（和设备码的验证页一样，只打开微软域名上的 `https` 地址），再把有效期（10 分钟）返回给前端。
2. 用户在浏览器里登录并同意，浏览器被重定向到 `http://localhost:<端口>/?code=…&state=…`。
3. 监听器只接受**一个**合格的请求：`GET /?…`，`Host` 是回环地址加这个端口（挡 DNS 重绑定），`state` 等于本次的值。其他请求（图标、别的页面、别的进程探测、`state` 不对）得到一个 404 / 400，等待继续，不会让登录失败，也不会被它完成。
4. 拿到授权码后，后端用 verifier 向令牌端点兑换（`grant_type=authorization_code`）。没有 verifier 的人拿到授权码也兑换不了。
5. 监听器在这次登录结束（成功、失败、取消、超时）后关闭。之后同设备码流程：Xbox → XSTS → Minecraft → 保存。

这开了一个监听端口，所以收得很小：只绑回环地址、临时端口、只活一次登录、每个请求最多读 8 KiB 且有读超时、授权码和 verifier 的 `Debug`/`Display` 都打码、错误里不含查询串。浏览器里看到的结果页不加载任何外部资源，并带 `no-store`、`no-referrer`。

失败分类（`reason`）：`expired`、`denied`、`network`、`service`、`storage`、`no-xbox-profile`、`child-account`、`region`、`not-entitled`、`no-profile`。界面把它们翻译成人话；不认识的分类显示后端的英文说明。

## 令牌放在哪

| 内容 | 位置 | 说明 |
| --- | --- | --- |
| 刷新令牌 | 系统凭据库（Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service） | `KeyringCredentialStore`；不回退到明文文件 |
| 账户信息（名字、UUID、Xbox 用户 ID） | `app_data/accounts/<uuid>.json` | 只有元数据，文件格式里没有令牌字段 |
| 微软访问令牌、Xbox、XSTS、Minecraft 令牌 | 不保存 | 只在内存里存在于一次登录或一次启动期间 |

- **令牌永远不经过 IPC。** 前端只能看到账户身份、设备码的展示值和失败分类。浏览器授权里，授权码、verifier、`state`、端口和授权网址也只在后端。
- 启动游戏时前端只传实例 ID 和账户 ID。后端用刷新令牌换新的微软令牌，再走一遍 Xbox 和 Minecraft，把新的 Minecraft 令牌交给游戏进程。刷新令牌每次使用都会轮换，新的会写回凭据库。
- 刷新令牌被撤销或过期时，后端返回 `microsoft-sign-in-required`，界面提示重新登录；网络故障不会被当成登录失效。
- 所有错误文本都不含响应体、令牌或完整 URL（只有主机名）。

## 前端的 IPC

| 命令 | 作用 |
| --- | --- |
| `microsoft_login_available` | 这个构建有没有配置应用 ID |
| `begin_microsoft_login` | 申请设备码并开始后台轮询，同一时间只允许一个 |
| `begin_microsoft_browser_login` | 打开微软登录页并等浏览器跳回本机（授权码 + PKCE），同一时间只允许一个登录（和设备码共用）。返回只有有效期 |
| `cancel_microsoft_login` | 取消正在进行的登录（两种方式都适用） |
| `open_microsoft_verification_page` | 打开本次登录的页面（设备码的验证页或浏览器授权的登录页）。网址由后端决定，且只接受微软域名上的 `https` 地址 |
| `list_microsoft_accounts` | 已登录的账户（只读元数据，不会访问凭据库） |
| `remove_microsoft_account` | 退出账户：删除凭据库里的刷新令牌和元数据 |

`start_installed_instance` 多了一个可选的 `account_id`：有它就用该微软账户启动，忽略玩家名和 UUID。

## 申请流程（维护者要做的）

### 1. 注册 Azure 应用（几分钟）

1. 打开 [Azure 门户](https://portal.azure.com) → **Microsoft Entra ID** → **应用注册** → **新注册**。需要一个微软账户；个人账户即可，不需要付费订阅。
2. 名称填 `Aether Launcher`。**受支持的账户类型**选「仅个人 Microsoft 账户」（或「任何组织目录中的账户和个人 Microsoft 账户」）。启动器固定使用 `consumers` 终结点，`XboxLive.signin` 范围只能用它。重定向 URI 现在可以留空；只有要用「浏览器授权」时才需要，见第 3 步。
3. 注册后进入 **身份验证（Authentication）** → **高级设置** → 把「允许公共客户端流」设为**是**，保存。设备码流程要求它。
4. **不要**创建客户端密码。这是公共客户端，密码放在发行版里不安全，也用不到（PKCE 就是替代它的）。
5. **要支持浏览器授权**：在 **身份验证** → **添加平台** → **移动和桌面应用程序**，自定义重定向 URI 填 `http://localhost`（不带端口，微软对回环地址会忽略端口）。不登记的话，浏览器授权会在微软的页面上报 `AADSTS50011`（回调地址不匹配）；设备码不受影响，界面也提供「改用设备码重试」。
6. 在应用的概述页复制**应用程序(客户端) ID**，一个 GUID。它是公开信息，不是秘密。

通常到这里设备码和 Xbox 两步就能成功（`XboxLive.signin` 是登录时请求的范围；如果门户提示缺少权限，在「API 权限」里确认已授予它）。我没有用真实的应用 ID 验证过这一步。

### 2. 向 Mojang 申请使用 Minecraft API 的权限（已通过）

**2026-10-31：Mojang 回复，这个应用 ID 已加入 API 允许名单，这一批没有拒绝项。** 他们只负责把应用 ID 加进名单，不提供接入咨询，也不排查 API 访问问题。下面是当时的申请流程，留作换应用 ID 时参考。

新注册的 Azure 应用**默认不能调用 Minecraft 服务**：`login_with_xbox` 会返回 `403`，内容是 `Invalid app registration, see https://aka.ms/AppRegInfo`。需要提交 Mojang 的应用审核表单：

- 表单：<https://aka.ms/mce-reviewappid>（出自 Minecraft Wiki 的 [Microsoft Authentication Scheme](https://minecraft.wiki/w/Minecraft_Wiki:Projects/wiki.vg_merge/Microsoft_Authentication_Scheme) 一页，Mojang 帮助中心也有说明）
- 要填的是应用名称、应用（客户端）ID、租户 ID（用上一步概述页里的「目录(租户) ID」）、启动器的用途和主页、联系人信息。
- 审核由 Mojang 做，**耗时没有公开的承诺**，通过后同一个应用 ID 即可使用，不需要改代码。

> 不确定的地方：我没能找到 Mojang 公布的审核标准和时限，也不知道只面向个人或非商业的启动器是否有额外要求。微软问答里有人说 `XboxLive.signin` 需要走 ID@Xbox 项目，但那是针对 Xbox 服务的，Minecraft 启动器按上面的表单走；如果表单被退回或另有要求，以 Mojang 的回复为准。

**不要借用别家启动器的应用 ID。** 那是在冒用他们的注册，违反他们和 Mojang 之间的条款，也会在他们被限制时连带失效。

### 3. 把应用 ID 配进构建

- **本地开发**：`$env:AETHER_MS_CLIENT_ID = "<应用 ID>"; npm run tauri dev`（运行时的环境变量优先于编译时的值）。
- **本地发行构建**：同样设置环境变量后 `npm run tauri build`，值会在编译时写进程序。
- **GitHub 发行**：仓库 Settings → Secrets and variables → Actions → **Variables** 新建 `AETHER_MS_CLIENT_ID`（是变量不是密钥，它是公开的）。`release.yml` 已经把它传给构建。

不合法的值（不是 GUID）会被当作没配置，登录按钮保持灰色。

### 4. 验证

用拥有 Minecraft 的账户登录一次，确认账户出现在列表里、启动游戏后进入正版服务器。如果登录卡在最后一步，界面显示「微软或 Mojang 的登录服务拒绝了这次登录」，那就是 403：应用 ID 不在允许名单里（换了新的应用 ID 之后需要重新申请）。

**已验证（2026-10-31）：** 用真实账户登录成功，账户出现在列表里。第一次尝试失败过，原因不是允许名单：XSTS 对 `rp://api.minecraftservices.com/` 只返回 `uhs`，没有 `xid`，旧的解析要求 `xid`，于是每个账户都停在「Xbox 服务的回复缺少必需的字段」。现在 `xid` 可选（`aether_auth::xbox::parse_xsts_response`），缺 `uhs`、令牌或 `xui` 仍然报错。

**还没验证：** 登录之后启动游戏并进入正版服务器；两种登录方式（设备码、浏览器授权）各自是否都走通，这里没有分别记录。

## 浏览器里测试界面

`npm run dev` 在普通浏览器里打开时没有 Tauri，所以界面里的实例、版本目录和登录都是空的。开发服务器会自动装一个假后端（`src/dev/mockBackend.ts`），整个流程能走通：

- 登录大约 6 秒后"成功"，账户是虚构的；设备码和浏览器授权两种方式都一样；
- `?mock=login-denied`、`?mock=login-child`、`?mock=login-expired` 让登录以对应的原因失败；
- `?mock=no-login` 模拟没有配置应用 ID 的构建；
- `?mock=off` 关掉假后端，回到没有后端的原始预览。

假后端只在开发服务器里加载，不会进入生产构建，里面没有任何真实令牌。

## 待定

- 浏览器授权目前只绑 `127.0.0.1`；`localhost` 若被浏览器先解析成 `::1` 会回退到 IPv4，没有同时监听 `[::1]`。遇到不回退的浏览器再加。
- 把登录页面嵌进应用（内置 WebView）而不是用系统浏览器：不做，系统浏览器里已有的会话、密码管理器和安全键更可靠。
- 皮肤和头像：账户卡片目前只显示首字母。
- 多账户同时在线、按实例绑定账户。
