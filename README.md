# Aidoku Sources

适用于 Aidoku 的中文漫画自定义书源。书源以 Rust/WASM 编写，打包为 `.aix`，并通过 GitHub Actions 发布书源列表。

## 书源

| 书源 | ID | 网站 | 版本 |
| --- | --- | --- | ---: |
| 55Comics | `zh.55comics` | https://www.55comics.com | 3 |
| 污污漫画 | `zh.comicbox` | https://www.comicbox.xyz | 7 |
| K漫画 | `zh.kmh001` | https://kmh001.com | 3 |
| 开心看漫画 | `zh.kxmanhua` | https://kxmanhua.com | 3 |
| 鸟鸟韩漫 | `zh.nnhanman` | https://nnhanman.xyz | 12 |
| 肉漫屋 | `zh.rouman5` | https://rouman5.com | 9 |

这些书源在元数据中标记为 `contentRating: 2`。如果 Aidoku 中看不到书源，请检查语言偏好和 NSFW 书源显示设置。

## 安装

在 Aidoku 中添加以下书源列表 URL，然后从列表安装书源：

```text
https://raw.githubusercontent.com/Jacky0725/aidoku-source/gh-pages/index.min.json
```

如果仓库已启用 GitHub Pages，也可尝试以下地址（本次未验证其可访问性）：

```text
https://jacky0725.github.io/aidoku-source/index.min.json
```

书源列表由 `.github/workflows/build.yaml` 在 `main` 分支更新后构建并发布至 `gh-pages`。外部书源由本站点维护者提供，并非 Aidoku 官方书源。

## 检查记录（2026-11-10）

本次检查了源代码、`source.json`、本地 `.aix` 包、发布列表和各书源的网页入口。6 个书源均可在本地通过 `aidoku package` 构建；构建后的包均通过 `aidoku verify`，包括 WASM 最低版本与必要导出、128×128 不透明图标、元数据 schema 检查。本地发布列表包含全部 6 个书源。

| 书源 | 网页入口检查 | 结论 |
| --- | --- | --- |
| 55Comics | 列表入口返回 Cloudflare HTTP 403 | 受站点验证限制，需在 Aidoku 实机验证 Cloudflare 处理 |
| 污污漫画 | 本次未复测 | 先前检查返回 HTTP 403；需在 Aidoku 实机验证 |
| K漫画 | `/home` 返回 Cloudflare HTTP 403 | 受站点验证限制，需在 Aidoku 实机验证；代码中的列表 URL 不随页码变化 |
| 开心看漫画 | 列表、详情、章节页返回 HTTP 200 | 抽查章节能找到图片元素；待实机验证 |
| 鸟鸟韩漫 | 列表、详情、章节页返回 HTTP 200 | 抽查章节能找到图片元素；待实机验证 |
| 肉漫屋 | 列表、详情、章节页返回 HTTP 200 | 已适配章节页的 `imagePaths` 数据；抽查取得 234 张图片，首张图片返回 HTTP 200；待实机验证 |

`aidoku verify` 只检查包结构和元数据，不会运行站内搜索、详情、章节或图片加载。HTTP 200 也不能证明书源能在 Aidoku 中完整阅读。本次未进行 Aidoku 实机端到端测试。55Comics 与 K漫画的 Cloudflare 验证可能因设备、网络和 Aidoku 版本而异；建议先使用包含 Cloudflare 修复的新版 Aidoku。开心看漫画的元数据未设置 `minAppVersion`，但通过了当前 CLI 的 schema 验证；其最低兼容版本应通过实机确认。

## 开发与验证

先安装 Rust、`wasm32-unknown-unknown` target 和 Aidoku CLI：

```powershell
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
```

在单个书源目录中构建并验证：

```powershell
cd sources/zh.nnhanman
aidoku package
aidoku verify package.aix
```

Aidoku 官方文档：[安装外部书源](https://aidoku.app/help/guides/getting-started/)、[书源开发工具](https://github.com/Aidoku/aidoku-rs)。
