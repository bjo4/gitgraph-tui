# gitgraph-tui

[![CI](https://github.com/bjo4/gitgraph-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/bjo4/gitgraph-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bjo4/gitgraph-tui)](https://github.com/bjo4/gitgraph-tui/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**終端機裡的 VSCode Git Graph。** 快速、唯讀的 git 歷史檢視器：彩色分支線、
commit 詳情、diff、搜尋——而且絕對不會寫入你的 repository。

[English README](README.md)

```text
┌ my-repo — all branches — 128/500 commits ────────────────────────────┐
│ ●    Uncommitted changes (1 files)                                   │
│ ●─╮  [HEAD] [main] [v1.0] merge: dev into main anna a1b2c3d 2h       │
│ ● │  fix: main work                       anna d4e5f6a 5h             │
│ │ ●  [dev] feat: dev work                 ben  0f1e2d3 1d             │
│ ●─╯  init                                 anna 9a8b7c6 2d             │
├──────────────────────────────────────────────────────────────────────┤
│ commit a1b2c3d · anna <anna@example.com> · 2026-07-06 14:30          │
│  M src/lib.rs  +12 -3                                                │
└ j/k:move g/G:top/bot tab:focus enter:diff /:search b:branches c:changes q:quit ┘
```

## 功能

- **彩色分支圖** — lane 分配演算法，正確處理分叉、合併、octopus merge 與交錯歷史，
  並保留清楚的交會線條
- **Ref 標籤** — 本地/遠端分支、tag、HEAD 直接顯示在列上
- **清單短 hash 欄位** — 每列在作者與相對時間旁顯示縮短版 commit hash
- **Commit 詳情** — 完整訊息、作者、日期、變更檔案（+/- 行數）
- **全螢幕 diff** — 逐檔、上色、可捲動
- **增量搜尋** — 訊息/作者/hash；`n`/`N` 自動載入更舊的歷史直到下一個符合
- **分支篩選** — 只看某條分支可達的 commit
- **分支 changes 視圖** — 選擇特定 branch 篩選後可按 `c` 開啟分割畫面，
  左側依 `Added` 與 `Not added` 顯示 staged 與 unstaged / untracked 檔案，
  右側顯示所選檔案的 diff
- **未提交變更** — 最新 commit 上方的即時狀態列
- **即時自動更新** — 其他 terminal 的 commit、切換分支、branch/tag 變動，以及工作區
  檔案異動都會自動反映，無需按鍵；游標與搜尋條件在刷新後仍保留
- **大型 repo 友善** — 歷史隨捲動以每 300 筆分塊載入
- **設計上唯讀** — 沒有 checkout、merge、reset。永遠不會有。

## 安裝

### 一行指令（Linux 與 macOS）

```sh
curl -fsSL https://raw.githubusercontent.com/bjo4/gitgraph-tui/main/install.sh | sh
```

從最新 GitHub release 下載對應平台的預編譯執行檔、驗證 sha256、
安裝到 `~/.local/bin`。

- 指定安裝目錄：`GITGRAPH_INSTALL_DIR=/usr/local/bin curl ... | sh`
- 指定版本：`GITGRAPH_VERSION=v0.3.0 curl ... | sh`
- 你的平台沒有預編譯檔？腳本會自動改用 `cargo install`
  從原始碼建置（需要 [Rust](https://rustup.rs)）。

### 用 cargo（所有平台，含 Windows）

```sh
cargo install --git https://github.com/bjo4/gitgraph-tui --locked
```

### 從原始碼

```sh
git clone https://github.com/bjo4/gitgraph-tui
cd gitgraph-tui
cargo install --path . --locked
```

## 使用

```sh
gitgraph-tui              # 目前目錄所在的 repository
gitgraph-tui ~/src/foo    # 指定路徑
```

小技巧：`alias gg=gitgraph-tui`

## 鍵位

| 鍵 | 動作 |
| --- | --- |
| `j` `k` `↑` `↓` | 移動（commit 列表；焦點在檔案列表時移動檔案） |
| `g` / `G` | 跳到頂端 / 底端（會載入完整歷史） |
| `Tab` | 切換焦點：commit ↔ 變更檔案 |
| `Enter` | 開啟選中檔案的 diff |
| `/` | 增量搜尋（訊息、作者、hash） |
| `n` / `N` | 下一個 / 上一個符合（自動載入更舊的 commit） |
| `b` | 依分支篩選 |
| `c` | 在目前 branch 篩選下開啟 staged / unstaged changes 視圖 |
| `r` | 強制完整重新載入（畫面本來就會自動更新）|
| `u` | 有新版本時開啟更新提示 |
| `Esc` / `q` | 返回 / 離開 |

## 更新通知

啟動時 gitgraph-tui 會向 GitHub 查詢最新 release——做法是讀取
`/releases/latest` 的重導向（不會呼叫 `api.github.com`）。查詢結果會快取
24 小時在 `~/.cache/gitgraph-tui/update.txt`（或 `$XDG_CACHE_HOME/gitgraph-tui/`，
macOS 上是 `~/Library/Caches/gitgraph-tui/`），所以一天最多只會發出一次這樣的
請求。若沒有設定 `HOME`，則完全不會檢查。

同一個新版本第一次被偵測到時會跳出彈窗；之後提示列會把 `g/G:top/bot` 換成
`u:update`（`g`/`G` 按鍵仍然可用，只是暫時沒列在提示列上——Diff 與 Branch
changes 視圖的提示列仍會顯示 `g/G`），按 `u` 隨時可以重新開啟彈窗。

在彈窗中按 `y` 會下載對應平台的 release 資產、驗證其公開的 SHA-256
checksum、解壓縮、試跑新的執行檔並帶 `--version` 確認可以啟動，
最後才原子性地換掉正在執行的執行檔。換完之後要重新啟動 gitgraph-tui
才會套用新版本。

以下情況無法就地更新，彈窗會改成顯示應執行的指令而不是更新按鈕：
你的平台沒有預編譯資產（例如 Windows）、執行檔是用 `cargo install`
安裝的、或安裝目錄沒有寫入權限。

要完全關閉檢查：

```sh
export GITGRAPH_NO_UPDATE_CHECK=1   # 或加上 --no-update-check
```

偵測到 `CI` 環境變數時也會自動略過檢查。

## 運作原理

三層嚴格分離：`src/git/` 透過 libgit2 讀取 repository 並只輸出純資料型別；
`src/graph/layout.rs` 是純函式 lane 分配引擎，把 commit DAG 轉成上色格子
（無 IO、單元測試完整覆蓋）；`src/app.rs` + `src/ui/` 是 Elm 式狀態機配
ratatui 視圖。開發者導覽見 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 貢獻

歡迎 Issue 與 PR — 見 [CONTRIBUTING.md](CONTRIBUTING.md)。
簡短版：`cargo test`、`cargo clippy --all-targets -- -D warnings`、
`cargo fmt --check` 三個都要綠。

## 授權

[MIT](LICENSE)
