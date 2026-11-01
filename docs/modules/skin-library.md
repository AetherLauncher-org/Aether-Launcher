# 皮肤库

账户页的「皮肤库」标签：本机保存的皮肤，每张画成正对着看的整个人。可以从文件或玩家名添加，改名、选纤细或经典手臂、删除。命令在 `src-tauri/src/commands/skin_library.rs`，规则在 `aether_app::skin_library`，图片检查在 `aether_content::skin`。

## 存什么、存在哪

`<数据目录>/skins/` 里，每张皮肤是 `<SHA-256>.png`，旁边一个 `index.json`（名字、模型、来源、添加时间）。按图片内容的哈希命名，所以同一张图加两次只有一张（结果里 `added: false`）。库里最多 500 张。前端用皮肤的 id（那个哈希）指定皮肤，**不传路径**。

## 来源

- **文件**：后端开系统对话框选一张 PNG（`add_skin_from_file`，页面不传路径，取消返回 `null`），文件名（去掉 `.png`）当皮肤名。
- **玩家名**：`add_skin_from_player`。名字必须是 3–16 个字母、数字或下划线；先 `GET api.minecraftservices.com/minecraft/profile/lookup/name/<名字>`（不要令牌）得到玩家 id，再走头像用的那条链（会话服务器 → `textures.minecraft.net`），皮肤里的 `metadata.model` 为 `slim` 就记成纤细手臂。只连这三个地址，HTTPS、不跟随重定向。
- **下载页**：下载页「光影」后面的「皮肤」标签，按玩家名从 Mojang、LittleSkin 或 Ely.by 查询，显示正面全身的 2D 预览。可「加入皮肤库」（新条目的 `source: download`）或「保存到文件夹…」。NameMC、Skindex、Planet Minecraft 只提供「在浏览器里打开」，用户自行保存 PNG，再用页面上的「添加皮肤文件」导入；不抓取网页。

### 下载页的接口与安全边界

规则在 `aether_app::skin_sources`，Tauri 只转换参数。命令：

- `search_skin { source, name }`：`source` 只收 `mojang`、`littleskin`、`elyby`，名字为 3–16 个字母、数字或下划线。返回 `{ token, source, name, slim, image }`，`image` 是后端验过的 `data:` PNG。
- `add_downloaded_skin { token }`：将预览的同一份 PNG 入库，来源记为 `download`；重复内容保留原条目的来源和名字。
- `save_downloaded_skin { token }`：后端开原生文件夹对话框，取消返回 `null`，成功只返回 `{ fileName }`。文件夹路径不进 IPC；文件名由已验证的玩家名产生，并处理 Windows 保留名。使用 `create_new`，已有同名文件或链接就报错，不覆盖；用户可重新选择文件夹。
- `open_skin_site { site }`：只收 `namemc`、`skindex`、`planetminecraft`，分别通过现有系统浏览器机制打开固定地址 `https://namemc.com/`、`https://www.minecraftskins.com/`、`https://www.planetminecraft.com/skins/`。不接收 URL。

预览 PNG 留在本会话的内存里，最多 8 份；达到上限时清空旧预览，旧令牌失效后需重新搜索。入库和保存可以使用同一个令牌，不重新联网取一张可能已改变的皮肤。

使用的公开接口：

| 来源 | 玩家名 → UUID | UUID → 贴图信息 | 允许的贴图地址 |
| --- | --- | --- | --- |
| Mojang | `GET https://api.minecraftservices.com/minecraft/profile/lookup/name/<name>` | `GET https://sessionserver.mojang.com/session/minecraft/profile/<uuid>` | `https://textures.minecraft.net/texture/<hex>` |
| LittleSkin | `POST https://littleskin.cn/api/yggdrasil/api/profiles/minecraft`，JSON 数组里只放这一个名字 | `GET https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/<uuid>` | `https://littleskin.cn/textures/<hex>` |
| Ely.by | `GET https://authserver.ely.by/api/users/profiles/minecraft/<name>` | `GET https://authserver.ely.by/session/profile/<uuid>` | `https://ely.by/storage/skins/<hex>.png`，以及文档所述正版代理返回的 `textures.minecraft.net` 地址 |

每次请求全局超时 15 秒，JSON 最多 64 KiB，Base64 解出的纹理 JSON 最多 16 KiB，PNG 最多 64 KiB。JSON 使用 serde 的默认递归深度限制。名字查询得到的 UUID 必须和档案回答里的 `id` 一致，回答里的玩家名仍按玩家名格式检查。所有网络请求只用 HTTPS，完全不跟随重定向；返回的纹理 URL 必须再次通过所属来源的精确主机和路径检查，拒绝端口、用户信息、查询参数、片段和路径穿越。文档或服务仍返回的白名单主机 `http` 纹理地址只升级为同主机 `https` 再请求，不发送 HTTP 请求。PNG 只检查文件头，不在 Rust 解码。

Mojang 纹理 URL 最后一段按 PNG 原始字节的 SHA-256 校验，包括 Ely.by 返回的 Mojang 纹理。LittleSkin / Ely.by 的文档没有承诺其自有纹理名的散列算法，因此不把这些名字解释成 SHA-256 或 MD5；信任边界为官方 HTTPS API、固定纹理主机和 PNG 检查，不使用渲染镜像作为信任根。

调研依据（只参考接口说明，未复制第三方实现或测试数据）：[LittleSkin API 手册](https://manual.littleskin.cn/advanced/api)、[LittleSkin Yggdrasil 地址](https://manual.littleskin.cn/yggdrasil/)、手册链接的 [authlib-injector 协议说明](https://yushijinhun.github.io/authlib-injector/en/yggdrasil-server-technical-specification.html)、[Ely.by Mojang 兼容 API](https://docs.ely.by/en/api.html)、[Ely.by 档案请求](https://docs.ely.by/en/minecraft-auth.html#profile-request)、[Ely.by 皮肤系统](https://docs.ely.by/en/skins-system.html)。Ely.by 的旧皮肤系统文档以 HTTP 为基址，本实现使用其另行记录的 HTTPS 两步接口，不请求会重定向到 HTTP 的旧 PNG 入口。

## 检查

每张图在保存前和每次读出来时都过 `validate_skin_png`：PNG、64×64 或旧版 64×32、64 KiB 以内，**只看文件头，不解码**。`index.json` 也是不可信的：读的时候限 1 MiB，只留 id 格式对的、不重复的、不超过上限的条目，名字限长清洗，来源不认识的改成 `file`。读写索引用同一把锁串行，写文件走临时文件再改名，不跟随放在目标名上的链接。图片被换成别的东西就不给界面。

## 换上

卡片上的「换上」打开账户选择框，列出已登录的微软账户（离线账户没有皮肤服务，不列出），可多选；只有一个账户时默认选中它，点一次「换上」确认。对选中的每个账户各换一次，一个失败（要重新登录、Mojang 不接受、请稍后再试）不影响其他账户，原因按账户显示。

前端只传两个 id（皮肤、账户），命令是 `wear_skin`。后端 `aether_app::skin_wear::wear`：从皮肤库读出图（再验一次）和模型，用账户的刷新令牌换一个短期 Minecraft 令牌（和启动游戏同一条路径，刷新令牌会轮换并存回系统凭据库），向 `POST api.minecraftservices.com/minecraft/profile/skins` 上传 multipart（`variant`：`classic` 或 `slim`，`file`：PNG）。令牌只进这个请求的 `Authorization` 头，不进返回值、日志和 `Debug` 输出；`aether-auth` 的 `HttpRequest` 新增二进制请求体（`bytes`），`Debug` 里只显示长度。multipart 的分隔串取一个不出现在图里的值。返回给界面的只有中文短句。

## 预览

旧皮肤（64×32）的预览按右边镜像出左边，和游戏里一样；皮肤库和下载页都使用 2D 全身预览，没有 3D 预览。
