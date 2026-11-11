# Aidoku Sources

适用于 Aidoku 的中文漫画自定义书源。书源以 Rust/WASM 编写，打包为 `.aix`，并通过 GitHub Actions 发布书源列表。

## 书源

| 书源 | ID | 网站 | 版本 |
| --- | --- | --- | ---: |
| 55Comics | `zh.55comics` | https://www.55comics.com | 3 |
| 污污漫画 | `zh.comicbox` | https://www.55comic.com | 8 |
| 禁漫天堂（18mh） | `zh.18mh` | https://18mh.net | 2 |
| K漫画 | `zh.kmh001` | https://kmh001.com | 4 |
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

此前检查了源代码、`source.json`、本地 `.aix` 包、发布列表和各书源的网页入口。原有 6 个书源均可在本地通过 `aidoku package` 构建并通过 `aidoku verify`。本次新增 18mh，并重新打包验证了污污漫画和 K漫画；发布列表会在推送到 `main` 后由 GitHub Actions 重新生成。

| 书源 | 网页入口检查 | 结论 |
| --- | --- | --- |
| 55Comics | 列表入口返回 Cloudflare HTTP 403 | 受站点验证限制，需在 Aidoku 实机验证 Cloudflare 处理 |
| 污污漫画 | `comicbox.xyz` 与 `55comic.com` 均返回 Cloudflare HTTP 403 | 两个域名展示同一站点；主地址改为 `55comic.com`，但换域名不能保证通过验证 |
| 禁漫天堂（18mh） | 列表、详情、章节与图片请求均返回 HTTP 200 | 网站图片为 AES-CBC 加密数据，已为封面和阅读页加入解密；抽样解密后均为可识别的 JPEG，待 Aidoku 实机验证 |
| K漫画 | 本地网络请求 `/home` 返回 Cloudflare HTTP 403 | 用户反馈手机浏览器可正常打开；已移除列表封面缺失时逐本请求详情页的慢路径，待 Aidoku 实机验证 |
| 开心看漫画 | 列表、详情、章节页返回 HTTP 200 | 抽查章节能找到图片元素；待实机验证 |
| 鸟鸟韩漫 | 列表、详情、章节页返回 HTTP 200 | 抽查章节能找到图片元素；待实机验证 |
| 肉漫屋 | 列表、详情、章节页返回 HTTP 200 | 已适配章节页的 `imagePaths` 数据；抽查取得 234 张图片，首张图片返回 HTTP 200；待实机验证 |

`aidoku verify` 只检查包结构和元数据，不会运行站内搜索、详情、章节或图片加载。HTTP 200 也不能证明书源能在 Aidoku 中完整阅读。本次未进行 Aidoku 实机端到端测试。`55comics.com`（55Comics）与 `55comic.com`（污污漫画备用网址）是两个不同域名。开心看漫画的元数据未设置 `minAppVersion`，但通过了当前 CLI 的 schema 验证；其最低兼容版本应通过实机确认。

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
