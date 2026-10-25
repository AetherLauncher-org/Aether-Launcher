# Contributing to Aether Launcher

感谢你愿意改进 Aether Launcher。

## 开始之前

- Bug、功能建议和下载源问题请使用对应的 Issue Form。
- 较大的交互或兼容性改动应先在 Discussions 说明目标、边界和替代方案。
- 安全漏洞不要提交公开 Issue，按照 [SECURITY.md](SECURITY.md) 私密报告。

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

## 提交与 PR

推荐使用 Conventional Commits：

```text
feat(download): add immutable artifact plan
fix(runtime): reject incompatible Java major version
docs(auth): document credential storage rules
```

PR 描述必须说明改动原因、验证方法、实际证据。UI 改动还应附截图或录屏。

提交代码即表示你同意按照仓库的 Apache-2.0 许可证发布贡献。
