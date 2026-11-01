# 整合包安装

从 Modrinth 装一个 `.mrpack` 整合包，为它建一个**独立的新实例**。搜索整合包在下载页的「整合包」标签页（跟在「光影」后面），也可以装本地 `.mrpack` 文件。

## 格式

`.mrpack` 是一个 ZIP，里面有：

- `modrinth.index.json`：格式说明（`formatVersion: 1`、`game: "minecraft"`、`dependencies`、`files[]` 等）；
- `overrides/`：覆盖到实例游戏目录的文件（`config/`、`options.txt` 之类）；
- `client-overrides/`：只对客户端生效的覆盖，按文件名**遮蔽** `overrides/` 里的同名文件；
- `server-overrides/`：只对服务端，客户端安装忽略。

格式说明见 Modrinth 的 [Modrinth Modpack Format](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack)。本实现只认 `formatVersion: 1`，`game` 必须是 `minecraft`，依赖里必须有一个 `minecraft` 版本和（可选）一个加载器（`fabric-loader` / `quilt-loader` / `forge` / `neoforge`）。

## 分工

- `aether-content::modpack` 只做**解析和校验**，不碰 I/O：大小/深度上限、`formatVersion`、依赖键、`files[]` 里的路径、`sha1`+`sha512`、下载地址主机白名单、路径安全（拒绝 Windows 保留名、`..`、分隔符、控制字符、超长等）。`safe_path` 和 `allowed_download` 是这里对外的主要入口。
- `aether-app::modpack` 做**有界 ZIP 检查 + 校验过的落盘**：`Pack::open` 先逐条验证归档（条目数、单条和总大小、符号链接、重复名、目录/文件冲突、加密条目），`Pack::materialize` 把文件按哈希校验后写进一个**必须不存在**的新目录，`client-overrides` 已在 `open` 时解析成最终层。下载经 `FileSource` trait 注入，生产实现 `PackNet` 只允许白名单主机、每次重定向都重新校验。
- `aether-app::modpack::install` 编排整件事：装原版（和加载器，如果整合包声明了）→ 建实例 → 落盘整合包文件 → 激活 → 失败回滚。前端只传**项目 id 或「本地」**、实例名、是否装可选文件；本地文件的路径由后端经 `Host::pick_file` 选，绝不跨 IPC。

## 流程

1. 选来源：给了 `projectId` 就取该项目最新合适的 `.mrpack`（发布版优先于测试版、优先 `primary` 文件、必须同时有 sha1 和 sha512）；否则后端弹原生文件选择框，前端拿不到路径。选择框被关掉按取消处理（`cancelled`）。
2. 下载整合包本身，**按平台给的 sha1 + sha512 校验**后暂存到私有文件（校验值就是信任根）。
3. `Pack::open` 校验整个 ZIP。
4. 装原版（和加载器，如果 `dependencies` 里有 `fabric-loader` / `forge` / `neoforge`），不建实例——复用 `install_game_files` 和 `modloader::execute::prepare_with`。
5. **最后**建实例（`aether_storage::create_instance`，初始为「未完成」）：先在数据目录下的私有暂存目录里建，把整合包文件写进实例自己的游戏目录，再 `activate_instance` 激活，全部成功后把整个实例目录**原子改名**进 `instances/` 作为唯一发布点。
6. 任何一步失败，暂存的实例目录会被删掉；已校验过的原版/加载器文件留在共享游戏树里，下次可复用。实例保持「未完成」直到整合包文件和激活都成功，失败不会留下一个指向错误内容、名字却写着整合包的实例。进度经 `Host::emit_install`（`install://progress`）发出；同一个取消令牌贯穿原版/加载器/整合包下载。

## 安全边界

- **ZIP 是不可信输入。** 条目数、单条和总大小、深度都有上限；符号链接、加密条目、`..`/`\`/盘符/Windows 保留名/尾随点空格、目录与文件同名、同名大小写别名全部拒绝；解压只按名字写到实例目录内，绝不逃出。
- **下载目标必须是白名单主机。** `allowed_download` 只放行 `cdn.modrinth.com`、`raw.githubusercontent.com`、`gitlab.com`，以及 `github.com` 的 `releases/download` 和 `raw` 路径；`https` 且无端口、无用户信息、无 `#`、`%` 转义合法、解码后的段不含 `.`/`..`/分隔符。重定向每次落地都重新过白名单。
- **哈希是信任根。** 每个 `files[]` 都必须有 40 位 sha1 和 128 位 sha512，下载流式校验，大小和哈希不符就丢。整合包本身用平台 API 给的哈希校验。
- **不覆盖已有文件。** 解压和下载都用「只在不存在时创建」，同名文件冲突直接报错。
- **路径永不跨 IPC。** 页面只传实例 id / 项目 id / 名字，本地文件由后端选。

## 还没做的 / 待定（需要维护者决定）

- **Quilt。** 整合包声明 `quilt-loader` 时会明确报错「Quilt 还不支持」，不会假装能装。加载器本身的 Quilt 元数据接口也没接（见 [modloader.md](modloader.md) 的「还没做的」）。
- **CurseForge 格式的整合包。** 只有 Modrinth 的 `.mrpack`；CurseForge 的 `manifest.json`（要 `x-api-key`）不识别，见 [content-metadata.md](content-metadata.md) 的「待定」第 1 条。
- **可选文件默认值。** 客户端可选（`client: optional`）的文件默认不装，页面上有开关勾选才装。
- **整合包文件没有独立进度阶段。** 整合包文件下载目前复用原版/加载器已有的进度事件，没有单独的阶段名；要更细的进度条是后续的事。
