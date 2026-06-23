# Aidoku Sources

Custom Aidoku 0.7+ sources for:

- 鸟鸟韩漫: https://nnhanman.xyz/
- 开心看漫画: https://kxmanhua.com/

## Usage

Add this verified source list URL in Aidoku:

```text
https://raw.githubusercontent.com/Jacky0725/aidoku-source/gh-pages/index.min.json
```

If GitHub Pages is enabled for the `gh-pages` branch, this shorter URL can also be used after it becomes available:

```text
https://jacky0725.github.io/aidoku-source/index.min.json
```

The GitHub Actions workflow builds each source package and publishes the generated source list to the `gh-pages` branch.

## Development

Aidoku sources are Rust/WASM packages built with `aidoku-cli`.

```powershell
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
cd sources/zh.nnhanman
aidoku package
```
