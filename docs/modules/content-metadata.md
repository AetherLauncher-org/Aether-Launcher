# 内容元数据（Modrinth）

实例管理页里，模组、资源包、光影包的卡片显示的是 Modrinth 上的项目名、简介、版本和图标，而不只是磁盘上的文件名。这篇讲它怎么识别、什么会离开本机、答案怎么当作不可信输入处理，以及还没定的事。

## 做什么

对 `mods`、`resourcepacks`、`shaderpacks` 里的**文件**（文件夹和存档不查）：

1. 算每个文件的 SHA-1；
2. `POST https://api.modrinth.com/v2/version_files`（`algorithm: sha1`）问 Modrinth 认识哪些哈希，得到版本和项目 id；
3. `GET https://api.modrinth.com/v2/projects?ids=[…]` 取这些项目的标题、简介、图标地址、类型；
4. 图标由后端从 `cdn.modrinth.com` 取下来，作为 `data:` URL 交给界面。

Modrinth 认不出的文件（自己改过的、别处下载的）照旧显示文件名。关掉的模组（`.jar.disabled`）字节没变，照样认得出来。

## 什么会离开本机

只有**文件的 SHA-1 哈希**（每次最多 100 个一批）和一个标识启动器的 `User-Agent`（`AetherLauncher/<版本> (+仓库地址)`，Modrinth 要求应用这样自报家门）。不发文件名、路径、实例名、实例 id、账户信息。Modrinth 因此能知道这个哈希对应的文件在文件夹里，别的不知道。

这一步默认开启，在内容页工具栏的「联网识别」开关里关掉（存在 `localStorage` 的 `aether.contentLookup`）。关掉后不发任何请求，卡片只显示文件名。

## 边界

- **前端不传文件名或地址。** `identify_instance_content` 只收实例 id 和一个固定标签（`mods` / `resourcepacks` / `shaderpacks`，别的标签后端拒绝）。`open_content_project` 只收 slug，页面地址 `https://modrinth.com/project/<slug>` 由后端拼，slug 只能是字母、数字、`-`、`_`、`.`，不以点开头。
- **只连两个地址。** `aether_app::identify::net::is_allowed_url`：`https://api.modrinth.com/v2/…` 和 `https://cdn.modrinth.com/…`，其他一律不连（包括 `@`、端口、空白、`http`）。不跟随重定向，不关闭证书校验。
- **答案是不可信输入。** `aether-content` 里：答案最多 4 MiB，JSON 解析有 serde 的递归上限；id 必须是字母数字、哈希必须是 40 位小写十六进制，项目类型只认 `mod`、`modpack`、`resourcepack`、`shader`；文字去掉控制字符并截断（标题 100、简介 300、版本 64 个字符）；图标地址必须是 `cdn.modrinth.com` 上的 HTTPS 地址；图片按开头几个字节认（PNG / JPEG / GIF / WebP），不信服务器声称的类型，**不收 SVG**（可能带脚本）；图标最多 256 KiB。
- **哈希就是身份。** 平台的答案只加一个标签，不改变一个文件是否可信，也不触发任何下载、安装或执行。这一步没有下载，所以不涉及「下载必须校验官方清单哈希」那条；将来从 Modrinth 下载内容时，期望哈希来自它的 API，见 [下载源调研](../excluded/download-sources.md) 第 9 节。
- **来源可见。** 识别出来的卡片有「Modrinth」按钮，指向项目页，不把第三方内容写成 Aether 自己的。
- **界面的内容安全策略不放宽。** 图标走 `data:`，`img-src` 里本来就有，`connect-src` 仍然只有 IPC。
- **不阻塞列表。** 列表先出，识别在后面单独取；识别失败（没网、限速、答案不对）不报错，只是不加标签，没连上时界面写一句「显示的是上次识别的结果」。

## 缓存

`<数据目录>/cache/modrinth.json`，大小上限 8 MiB，读到坏的就当没有：

- 按哈希存答案：认得的信 7 天，不认得的信 1 天，之后再问；
- 按「实例/类型/文件名」存哈希，文件大小和修改时间没变就不重算（大文件夹不用每次打开都读一遍）；
- 最多 5000 条，超了删最旧的；写入先写临时文件再 rename。
- 一次查询里任何一个请求失败，这一批都不写进缓存，不会把没问到的文件记成「不认识」。

图标在 `<数据目录>/cache/icons/<项目 id>`，每次读出来前再检查一遍大小和图片头，所以被别的东西改过的文件不会被当图片送给界面。一次最多新取 60 个图标，其余下次打开再取。

## 限制

- 每次最多查一个文件夹里的前 500 个文件，单个文件超过 512 MiB 不算哈希。
- Modrinth 按 IP 限速（文档写的是每分钟 300 次）。一次识别是 1 次 POST、每 100 个项目 1 次 GET，加图标，远低于限额；被限速（429）时按「没连上」处理，缓存继续用。
- 只有 Modrinth。CurseForge 没做，见下。

## 玩家皮肤（账户头像）

微软账户的头像画的是玩家皮肤的 3D 头（里层的头加外层的帽子层），不再是名字首字母。

- 后端 `account_skin(uuid)`：`GET https://sessionserver.mojang.com/session/minecraft/profile/<uuid>`（公开，不需要令牌，玩家 UUID 本来就是公开的）→ 解 `textures` 属性（base64 的 JSON）→ 取 `SKIN.url` → 从 `textures.minecraft.net` 下载 PNG。
- 同样的边界（`aether_content::skin`、`aether_app::skins`）：只连这两个地址，HTTPS、不跟随重定向；皮肤地址必须正好是 `textures.minecraft.net/texture/<十六进制>`（Mojang 给的 `http` 形式会改成 `https`）；两层 JSON 都有大小上限；图片最大 64 KiB，必须是 PNG，文件头里的尺寸必须是 64×64 或旧版的 64×32，**不解码就先看文件头**。给界面的是 `data:` URL。
- 缓存在 `<数据目录>/cache/skins/<uuid>.png`，一天；没有自定义皮肤的玩家记一个 `.none` 标记，也是一天；没网时旧皮肤照用。每次读出来再验一遍，被改过的文件不会送给界面。
- 取不到、没有皮肤、离线账户、这台机器没有 2D 画布：一律退回字母头像，不报错。
- 画法在前端 `src/lib/skinHead.ts`：头的顶面、正面、侧面（外层同样三个面，大 1/8）各用一个仿射变换贴到斜 45° 的立方体上，侧面加暗；不用 WebGL，不依赖任何库。

## 待定

这些是设计问题，不是实现细节，由维护者定：

1. **CurseForge。** 它的 API 要 `x-api-key`，[下载源调研](../excluded/download-sources.md)里写了 key 不能进桌面客户端、仓库或 CI 日志，所以要先定 key 放在哪里（自己的代理服务？每个用户自己填？）以及授权和来源标识的做法。定之前这里不接，也不会因为缺 key 去绕。接入时会按指纹（它自己的 MurmurHash2）查，`aether-content` 里 `Platform` 再加一项。
2. **默认开不开。** 现在默认开启，理由是不开就看不到名字，等于没有这个功能；代价是第一次打开内容页就会把文件夹里文件的哈希发给 Modrinth。如果要改成第一次问一下、或者默认关，改的是 `src/lib/contentLookup.ts` 的默认值和界面上的一句说明。
3. **整合包和数据包。** `modpack` 类型的项目会被认出来（整合包 `.zip` 放在资源包文件夹里的情况很少见，但会显示成整合包名），数据包在存档里面，现在不查。
4. **版本过期提示。** 能顺手告诉用户「这个模组有新版本」，但这是在下载功能之上的另一件事，没做。

## 改它之前

- 加新的平台答案字段，先决定界面要不要显示；显示的要过 `clean_text` 和长度上限，是地址的要有主机白名单。
- 新增要连的地址，只能改 `is_allowed_url`，并补测试证明别的地址仍被拒绝。
- 测试里不连真实的 Modrinth：`aether-app` 用假的 `ContentNet`，`aether-content` 只测解析。
