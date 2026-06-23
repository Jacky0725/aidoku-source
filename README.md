# Aidoku Sources

Custom Aidoku 0.7+ sources for:

- 鸟鸟韩漫: https://nnhanman.xyz/
- 开心看漫画: https://kxmanhua.com/

## Usage

After this repository is published with GitHub Pages enabled, add this source list in Aidoku:

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
