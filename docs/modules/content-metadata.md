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

微软账户的头像画的是玩家皮肤的头，正对着看：里层的脸加外层的帽子层（外层大 1/8，罩在外面一圈），不再是名字首字母。

- 后端 `account_skin(uuid)`：`GET https://sessionserver.mojang.com/session/minecraft/profile/<uuid>`（公开，不需要令牌，玩家 UUID 本来就是公开的）→ 解 `textures` 属性（base64 的 JSON）→ 取 `SKIN.url` → 从 `textures.minecraft.net` 下载 PNG。
- 同样的边界（`aether_content::skin`、`aether_app::skins`）：只连这两个地址，HTTPS、不跟随重定向；皮肤地址必须正好是 `textures.minecraft.net/texture/<十六进制>`（Mojang 给的 `http` 形式会改成 `https`）；两层 JSON 都有大小上限；图片最大 64 KiB，必须是 PNG，文件头里的尺寸必须是 64×64 或旧版的 64×32，**不解码就先看文件头**。给界面的是 `data:` URL。
- 缓存在 `<数据目录>/cache/skins/<uuid>.png`，一天；没有自定义皮肤的玩家记一个 `.none` 标记，也是一天；没网时旧皮肤照用。每次读出来再验一遍，被改过的文件不会送给界面。
- 取不到、没有皮肤、离线账户、这台机器没有 2D 画布：一律退回字母头像，不报错。
- 画法在前端 `src/lib/skinHead.ts`：先画里层的脸（皮肤图 (8,8) 起的 8×8），再在上面画外层的脸（(40,8)，铺满画布，里层在正中小 1/8），最近邻取样，不平滑，不加暗；没有顶面和侧面。不用 WebGL，不依赖任何库。

## 下载内容（Modrinth）

下载页的「模组与资源」可以搜索并安装模组、数据包、资源包和光影（`aether_content::browse`、`aether_app::content_install`，命令 `get_content_target` / `search_content` / `install_content`）。

- **搜索**：`GET /v2/search`，按类型（`project_type` 刻面）、游戏版本和加载器（只对模组）筛，排序、翻页（每页 20）。游戏版本、加载器、排序、类型都是固定集合或严格字符集，搜索词百分号编码，不能改变请求。默认按所选实例的游戏版本和加载器筛（从安装的版本 id 读出，见 `target_of`），可以关掉。
- **装哪个版本**：`GET /v2/project/{id}/version`，再**自己**挑：真的列出这个游戏版本（和加载器）的，正式版优先于 beta 优先于 alpha，同级取最新发布。不信平台的筛选。
- **落在哪**：由内容类型和校验过的实例记录决定——模组 `mods/`（`.jar`）、资源包 `resourcepacks/`、光影 `shaderpacks/`、数据包 `saves/<世界>/datapacks/`（后三者 `.zip`）。页面只传实例 id、项目 id、内容类型和世界文件夹名，**不传路径也不传下载地址**。游戏运行中拒绝。模组要实例有加载器；整合包不走这条链，在下载页的「整合包」标签页装成一个新实例（见 [modpack.md](modpack.md)）。
- **文件的检查**：只收地址在 `cdn.modrinth.com`（HTTPS，无用户信息、端口）、文件名是安全的纯文件名（没有路径分隔符、Windows 非法字符和设备名）、大小在 256 MiB 以内、带 SHA-1（给了 SHA-512 就也要对）的文件。下载时边写边算哈希，临时文件在目标文件夹里以独占新建方式写入，**大小和哈希都对才改成真名**，否则只清理本次创建的未完成文件。已有同名正式文件或临时文件不覆盖、不删除（也不跟随链接）。哈希来自平台的 API 回答，不是镜像。
- **前置**：模组可以一起装 `required` 的依赖（深度 3、一次最多 12 个，环会停）；装不上的前置在结果里列出原因，不影响主项目。`optional` 和 `incompatible` 不处理。
- **进度与排队**：每次安装先由后端生成不透明的操作 id。`begin_content_install` 启动任务并返回 id，`wait_content_install` 按 id 取最终结果，`cancel_content_install` 只取消该 id 的任务。进度经 `Host::emit_content_install` → `content-install://progress` 推送，包含 `operationId`、`phase`（`resolving` / `downloading` / `verifying` / `done` / `failed` / `cancelled`）、当前 `projectId` / `fileName`、单文件 `bytesDone` / `bytesTotal`、`filesDone` 和 `filesDiscovered`。普通进度每个任务最多约每 100 ms 一次，终态立即通知。依赖边装边发现，`filesDiscovered` 是目前已发现的项目数、只增不减；全部解析结束前显示「已装 n / ≥总数」，进度条表示当前文件的字节比例，不把暂时发现的数量当最终总量。界面最多同时运行两个任务，其余排队；排队或运行中的项目不能重复点安装，切换搜索条件或内容页不会丢失任务。
- **取消与保留**：取消在网络读取的块之间检查，正在等待的单次网络读取要等返回后才能停止。取消会移除本次独占创建的 `.aether-part` 和尚未完成的占位目标；本次已完整下载、通过大小与全部哈希检查并放到正式文件名的文件保留。最终结果用 `status` 区分成功、失败、取消，`installed` 精确列出保留的已验证文件（项目 id、纯文件名和版本），界面直接显示；已经结束的任务不会被迟到的取消改成取消。运行前已有的正式文件和临时文件均不覆盖、不删除。同名临时文件会令这次文件安装失败，留给用户处理。
- **没做**：CurseForge（见下面待定第 1 条）、选版本（总是取最新合适的）、装完后的更新检查。

## 保存到所选文件夹

内容卡片的「保存到文件夹…」与安装走同一套操作机制：`begin_folder_save` 生成不透明的操作 id 并进入同一个两槽队列，`wait_content_install` 取最终结果，`cancel_content_install` 取消，进度仍经 `content-install://progress` 推送。后端 `Host::pick_folder` 在任务线程里打开原生对话框，所选目录不经过 IPC。实例只提供游戏版本和加载器，复用同一套本地版本选择器、官方 API 哈希和 CDN 下载路径；不改实例文件，数据包不需要选择世界。用户在对话框里取消时任务以 `cancelled` 结束，不联网下载、不产生文件。

文件检查与安装完全一致：SHA-1 必需，平台提供 SHA-512 时同时检查，单文件上限 256 MiB，只接受 `cdn.modrinth.com`。先用 `create_new` 独占占用最终名称，再写临时文件、校验和重命名；同名正式文件或临时文件不覆盖、不删除。所选目录及其祖先不能是符号链接或 Windows reparse point；与现有实例文件操作一样，不支持其他进程并发替换目录组件。

「同时装必需的前置」开启时，模组前置保存到同一目录，仍限制依赖深度 3、最多成功保存 12 个文件。最终结果与安装共用 `ContentInstallResult`：`installed` 精确列出保留的已验证文件，`skipped` 列出未能保存的前置和原因，界面用同一张任务卡显示。

## 待定

这些是设计问题，不是实现细节，由维护者定：

1. **CurseForge。** 它的 API 要 `x-api-key`，[下载源调研](../excluded/download-sources.md)里写了 key 不能进桌面客户端、仓库或 CI 日志，所以要先定 key 放在哪里（自己的代理服务？每个用户自己填？）以及授权和来源标识的做法。定之前这里不接，也不会因为缺 key 去绕。接入时会按指纹（它自己的 MurmurHash2）查，`aether-content` 里 `Platform` 再加一项。
2. **默认开不开。** 现在默认开启，理由是不开就看不到名字，等于没有这个功能；代价是第一次打开内容页就会把文件夹里文件的哈希发给 Modrinth。如果要改成第一次问一下、或者默认关，改的是 `src/lib/contentLookup.ts` 的默认值和界面上的一句说明。
3. **整合包。** 已做：`modpack` 类型的项目能被搜到并在下载页的「整合包」标签页装成新实例（见 [modpack.md](modpack.md)）；数据包的识别（存档里面的）现在不查，只能装。
4. **版本过期提示。** 能顺手告诉用户「这个模组有新版本」，但这是在下载功能之上的另一件事，没做。

## 改它之前

- 加新的平台答案字段，先决定界面要不要显示；显示的要过 `clean_text` 和长度上限，是地址的要有主机白名单。
- 新增要连的地址，只能改 `is_allowed_url`，并补测试证明别的地址仍被拒绝。
- 测试里不连真实的 Modrinth：`aether-app` 用假的 `ContentNet`，`aether-content` 只测解析。
