# Contributing to Aether Launcher

感谢你愿意改进 Aether Launcher。

## 开始之前

- Bug、功能建议和下载源问题请使用对应的 Issue Form。
- 较大的交互或兼容性改动应先在 Discussions 说明目标、边界和替代方案。
- 安全漏洞不要提交公开 Issue，按照 [SECURITY.md](SECURITY.md) 私密报告。
- 如果你正在使用 AI 向该仓库贡献，请自行先 Review 一遍且标明使用的模型（例如 Claude Sonnet 5.5）。

## 开发流程

1. Fork 仓库并从 `main` 创建短生命周期分支。
2. 保持改动聚焦，让代码和 review 都更容易理解。
3. 为行为变化添加 Rust 单元测试或前端测试。
4. 用户行为、开发接口或兼容性发生变化时，直接更新相关文档。
5. 在本地运行与改动相关的检查，提交结果和未覆盖的部分。

```powershell
npm ci
npm run check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 不能破坏的规则

动手前先读 [AGENTS.md](AGENTS.md)。里面的安全不变量不是建议，不会为了让测试通过或功能好做而放宽：

- 前端只能通过 `src/ipc/` 里登记的窄化命令调用后端，不增加执行任意命令、读写任意路径的入口，也不把文件系统路径传给前端。
- 凭据（令牌、刷新令牌、设备码）不进序列化输出、日志、IPC 和持久化，加密存储除外。
- 下载目标必须是安全的相对路径，所有下载都校验哈希，哈希来自官方清单。镜像不是信任根。
- 清单、日志、崩溃报告、其他启动器的配置和用户选的文件都是不可信输入：解析要有大小和深度上限，不执行其中的内容，也不去执行扫描时碰到的可执行文件。

违反其中任何一条的 PR 会被要求修改，不会因为「功能能跑」而合并。

## 已有的设计

别人已经做过决定的地方，不要在一个功能 PR 里顺手推翻。例如交互方式、组件的行为、依赖方向。想改就先开 Discussion 或在 PR 里写明理由，等维护者确认；设计问题请写进 PR 描述的「待定」，不要自己拍板。

一个 PR 一个关注点。顺带的重构、格式化、默认值的改变请拆成单独的 PR。

## 使用 AI 辅助

可以用 AI 写代码，但提交的人对每一行负责：

- 提交之前自己读过、跑过每一行。能解释它为什么这样写，被问到时不用回去问模型。
- PR 描述写你**实际验证了什么、没验证什么**。没跑过的不要勾，没走过的真实流程（例如只验证了 Fabric，却在标题里写 Forge / NeoForge）不要写成已完成。
- 界面上写的承诺必须和后端行为一致：界面显示「会安装」，后端就要真的安装。
- 失败路径和重试路径也要测：安装中途失败会留下什么，重试会不会悄悄变成另一件事。
- 不要用 AI 生成大段注释、自我表扬式的 PR 描述或看起来完整但没验证过的测试。测试要能在代码出错时失败。

是不是 AI 写的不重要，没有被验证过的代码才是问题。

## 提交与 PR

请使用 Conventional Commits：

```text
feat(download): add immutable artifact plan
fix(runtime): reject incompatible Java major version
docs(auth): document credential storage rules
```

PR 描述必须说明改动原因、验证方法、实际证据，以及没有验证的部分。UI 改动还应附截图或录屏。

提交代码即表示你同意按照仓库的 Apache-2.0 许可证发布贡献。
