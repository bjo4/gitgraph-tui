# gitgraph-tui 開源發佈設計（curl 安裝 + 專案可用性優化）

日期：2026-07-06
狀態：已核准
前置：v0.1 已合入 main 並推送至 https://github.com/bjo4/gitgraph-tui（public）

## 1. 目標

1. 其他人可以用一行 curl 指令把 gitgraph-tui 裝到自己電腦（不需要 Rust 工具鏈）。
2. 專案具備標準開源配備：授權、CI、release 自動化、清楚的 README 與貢獻指南，讓陌生人能快速理解與上手。

### 非目標（本次不做）

- 發佈到 crates.io（需要使用者的 crates.io token；Cargo.toml metadata 先備妥，發佈列為後續選項）
- Windows 預編譯 binary（README 註明 Windows 用 `cargo install`）
- Homebrew formula / AUR / deb/rpm 套件
- cargo-binstall 支援

## 2. curl 安裝鏈

### install.sh（repo 根目錄）

使用方式：

```sh
curl -fsSL https://raw.githubusercontent.com/bjo4/gitgraph-tui/main/install.sh | sh
```

行為順序：

1. 偵測平台：`uname -s`（Linux/Darwin）+ `uname -m`（x86_64/aarch64/arm64），映射到 release target 名稱。
2. 取版本：預設查 GitHub API `releases/latest`；`GITGRAPH_VERSION` 環境變數可指定版本。
3. 下載 `gitgraph-tui-v<version>-<target>.tar.gz` 與對應 `.sha256`，用 `sha256sum`/`shasum` 驗證，不符即中止。
4. 解壓出 `gitgraph-tui` 單一 binary，安裝到 `${GITGRAPH_INSTALL_DIR:-$HOME/.local/bin}`（目錄不存在則建立）。
5. 檢查安裝目錄是否在 PATH，不在則印出加入 PATH 的提示；最後提示 `alias gg=gitgraph-tui`。
6. **Fallback**：平台無預編譯檔（或下載 404）時，若偵測到 cargo 就直接改走 `cargo install --git https://github.com/bjo4/gitgraph-tui --locked` 並事先印出「將從原始碼建置、約需數分鐘」的說明（curl|sh 情境 stdin 是 pipe，不做互動詢問）；沒有 cargo 則印出安裝 Rust 的指引後以非零碼結束。

品質要求：POSIX sh（`#!/bin/sh`、不用 bash 專屬語法）、`set -eu`、所有外部指令失敗都有可讀錯誤訊息、curl 與 wget 二擇一可用即可。

### 平台矩陣（release artifacts）

| Target | 執行環境 |
|---|---|
| `x86_64-unknown-linux-musl` | 任何 x86_64 Linux（靜態連結） |
| `aarch64-unknown-linux-musl` | ARM64 Linux |
| `x86_64-apple-darwin` | Intel Mac |
| `aarch64-apple-darwin` | Apple Silicon Mac |

命名：`gitgraph-tui-v<version>-<target>.tar.gz`，內含單一 `gitgraph-tui` binary 與 LICENSE、README.md。每個 tar.gz 旁附 `<同名>.tar.gz.sha256`（格式：`<hash>  <filename>`，可直接 `sha256sum -c`）。

### 配套程式碼調整：git2 依賴減肥

`Cargo.toml` 的 git2 改為：

```toml
git2 = { version = "0.21", default-features = false }
```

理由：本工具只讀本地 repo，用不到 git2 的 https/ssh 傳輸功能；砍掉後不再依賴 OpenSSL，musl 靜態編譯無需 vendored openssl、binary 更小、CI 更快。驗收：全部既有測試（80 個）維持綠。

## 3. GitHub Actions

### `.github/workflows/ci.yml`

- 觸發：push 到 main、所有 PR。
- Job `test`（ubuntu-latest）：checkout → Rust stable + cache → `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo test`。
- Job `check-macos`（macos-latest）：`cargo check --all-targets`（確保 mac 能編，不重跑全套測試以省時間）。

### `.github/workflows/release.yml`

- 觸發：push tag `v*`。
- Job `build`，matrix 對應平台矩陣四個 target：
  - Linux musl（ubuntu-latest）：x86_64 裝 `musl-tools` 直接編；aarch64 用 `cross` 編譯。
  - macOS：`macos-13`（x86_64）與 `macos-14`（aarch64）原生編。
  - 每個 target：`cargo build --release --locked --target <target>` → `strip`（Linux）→ 打包 tar.gz（含 binary、LICENSE、README.md）→ 產 `.sha256`。
- Job `release`（needs build）：收集所有 artifacts，`gh release create`（`GITHUB_TOKEN`）附自動 release notes 與 curl 安裝指令片段。
- 權限：workflow 宣告 `permissions: contents: write`（建 release 所需的最小權限）。

## 4. 開源文件

| 檔案 | 內容 |
|---|---|
| `LICENSE` | MIT 全文，`Copyright (c) 2026 bjo4` |
| `README.md`（全面改寫，英文） | badges（CI 狀態、latest release、license）→ 一段話定位 + 終端畫面示意（含顏色說明的 code block）→ Features 清單 → Install：curl 一鍵（主推）/ `cargo install --git` / from source → Usage → 完整鍵位表 → How it works（三層架構各一句話，連到 spec 與 CONTRIBUTING）→ Contributing 連結 → License |
| `README.zh-TW.md` | 繁中版，內容與英文版對應；兩版開頭互相連結 |
| `CONTRIBUTING.md` | 開發環境需求（Rust 1.88+）、`cargo test` / clippy / fmt 三個 gate、專案結構導覽（`src/git/` 邊界：git2 不外洩；`src/graph/layout.rs` 純函式引擎；`src/app.rs` Elm 式狀態機；`src/ui/` 只讀 state 畫畫面）、測試慣例（Fixture、TestBackend）、PR 檢查清單、指向 docs/superpowers/ 的 spec 與 plan |
| `CHANGELOG.md` | Keep a Changelog 格式；`[0.1.0]` 列 v0.1 功能集 |
| `Cargo.toml` 補 metadata | `repository`、`readme = "README.md"`、`keywords = ["git", "tui", "terminal", "graph", "log"]`、`categories = ["command-line-utilities", "development-tools"]` |

## 5. 發佈流程（實作完成後執行）

1. 全部變更 commit 到 main、push。
2. `git tag v0.1.0 && git push origin v0.1.0` → 觸發 release workflow。
3. 等 Actions 完成，確認 release 頁有 4 個 tar.gz + 4 個 .sha256。
4. 乾淨環境實測：`curl -fsSL .../install.sh | sh`，驗證裝出的 binary `--version` 可執行、TUI 可開。
5. 實測失敗則修到通過為止才算完成。

## 6. 驗收標準

- `curl -fsSL https://raw.githubusercontent.com/bjo4/gitgraph-tui/main/install.sh | sh` 在本機（linux x86_64）實測成功，`gitgraph-tui --version` 輸出正確版本。
- GitHub Release v0.1.0 含 4 平台 tar.gz 與 sha256，CI workflow 綠。
- `cargo test` 80 個測試全綠、clippy/fmt 乾淨（git2 減肥後）。
- LICENSE / README（雙語）/ CONTRIBUTING / CHANGELOG 齊備，文件內連結有效。
- install.sh 通過 shellcheck（如本機可用）與 `sh -n` 語法檢查。

## 7. 決策紀錄

| 決策 | 選擇 | 落選方案 |
|---|---|---|
| 發佈方式 | 預編譯 binaries + cargo fallback | 純原始碼建置腳本 |
| Linux 連結方式 | musl 靜態 | glibc 動態（發行版相容性差） |
| git2 features | default-features = false | 保留預設（拖 OpenSSL，靜態編譯麻煩） |
| README 語言 | 英文為主 + 繁中版 | 僅英文 |
| crates.io | 本次不發佈，metadata 備妥 | 立即發佈（需使用者 token） |
