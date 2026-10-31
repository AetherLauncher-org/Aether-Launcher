# 皮肤库

账户页的「皮肤库」标签：本机保存的皮肤，每张画成正对着看的整个人。可以从文件或玩家名添加，改名、选纤细或经典手臂、删除。命令在 `src-tauri/src/commands/skin_library.rs`，规则在 `aether_app::skin_library`，图片检查在 `aether_content::skin`。

## 存什么、存在哪

`<数据目录>/skins/` 里，每张皮肤是 `<SHA-256>.png`，旁边一个 `index.json`（名字、模型、来源、添加时间）。按图片内容的哈希命名，所以同一张图加两次只有一张（结果里 `added: false`）。库里最多 500 张。前端用皮肤的 id（那个哈希）指定皮肤，**不传路径**。

## 来源

- **文件**：后端开系统对话框选一张 PNG（`add_skin_from_file`，页面不传路径，取消返回 `null`），文件名（去掉 `.png`）当皮肤名。
- **玩家名**：`add_skin_from_player`。名字必须是 3–16 个字母、数字或下划线；先 `GET api.minecraftservices.com/minecraft/profile/lookup/name/<名字>`（不要令牌）得到玩家 id，再走头像用的那条链（会话服务器 → `textures.minecraft.net`），皮肤里的 `metadata.model` 为 `slim` 就记成纤细手臂。只连这三个地址，HTTPS、不跟随重定向。
- **下载页**：还没做。NameMC、Skindex、Planet Minecraft 没有官方接口，条款也没核实，**不自动抓取**，只会放「在浏览器里打开」的按钮，用户自己保存 PNG 再从文件加（见 [下一批功能](../design/next-features.md) 第 5 条）。

## 检查

每张图在保存前和每次读出来时都过 `validate_skin_png`：PNG、64×64 或旧版 64×32、64 KiB 以内，**只看文件头，不解码**。`index.json` 也是不可信的：读的时候限 1 MiB，只留 id 格式对的、不重复的、不超过上限的条目，名字限长清洗，来源不认识的改成 `file`。读写索引用同一把锁串行，写文件走临时文件再改名，不跟随放在目标名上的链接。图片被换成别的东西就不给界面。

## 没做

- **换上**：卡片上的「换上」按钮现在是灰的并写明原因。换肤要用微软账户的 Minecraft 令牌向 `api.minecraftservices.com/minecraft/profile/skins` 上传（multipart，`variant` 加 PNG），令牌只能在后端里用，而 `aether-auth` 现在只处理文本请求体，要先加二进制上传。有多个微软账户时弹出卡片可多选，对选中的每个账户各换一次；离线账户没有皮肤可换，不列出。
- 下载页的皮肤来源（上面）。
- 旧皮肤（64×32）的预览按右边镜像出左边，和游戏里一样；没有 3D 预览。
