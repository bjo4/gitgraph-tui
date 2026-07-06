# gitgraph-tui 設計文件

日期：2026-07-06
狀態：已核准（brainstorming 流程完成，使用者於 2026-07-06 核准設計）

## 1. 目標與定位

Rust + ratatui 的**只讀** git 歷史瀏覽 TUI，體驗對標 VSCode 的 Git Graph 插件：

- 彩色 branch graph（lane 線、分叉、merge 收攏）
- commit 列表：訊息、作者、相對時間
- branch / tag / HEAD 標籤（本地與遠端 branch）
- 選中 commit 顯示詳情：完整訊息、hash、作者、日期、變更檔案列表（+/- 行數）
- 單檔彩色 diff 檢視
- 增量搜尋（訊息 / 作者 / hash）與 branch 篩選
- graph 頂端顯示未 commit 的工作區狀態（synthetic row）

執行檔名 `gitgraph-tui`（README 建議 shell alias `gg`）。
用法：在 repo 內執行 `gitgraph-tui`，或 `gitgraph-tui /path/to/repo`。

### 非目標（v1 明確不做）

- 任何會改變 repo 狀態的 git 操作（checkout、merge、rebase、cherry-pick、stash……）——純只讀。
- 靜態輸出模式（`--no-tui` / pipe 偵測）。
- stash 列表顯示（只顯示工作區未 commit 狀態）。
- 設定檔 / 自訂主題。

## 2. 技術棧

| 項目 | 選擇 | 理由 |
|---|---|---|
| 語言 | Rust (edition 2021+) | 使用者已有 accountant-cli 經驗；單一靜態執行檔；大 repo 效能 |
| TUI | ratatui + crossterm | 生態最成熟（gitui、serie 同款） |
| git 存取 | git2（libgit2 綁定） | 成熟穩定、文件充足；gitoxide 評估過但 API 未穩定，落選 |
| CLI 參數 | clap（derive） | repo 路徑等參數 |
| 其他 | anyhow、time、unicode-width | 錯誤處理、時間格式、寬字元對齊 |

## 3. 架構（Elm 式單向資料流）

單一 `AppState` + 事件迴圈 + `update()` + `view()`；狀態變更全部集中。

```
src/
├── main.rs          進入點：clap 解析參數、terminal 初始化/還原、事件迴圈
├── app.rs           AppState、Mode（Normal / Search / Diff / BranchFilter 彈窗）、update(event) 純狀態轉移
├── event.rs         crossterm 輸入事件 + tick 封裝
├── git/
│   ├── mod.rs
│   ├── repo.rs      開 repo、revwalk（topo+time、分塊 lazy load）、refs、工作區狀態
│   ├── diff.rs      commit 變更檔案列表（vs 第一個 parent）、單檔 diff 行
│   └── types.rs     CommitInfo / RefInfo / FileChange 等純資料結構
├── graph/
│   ├── mod.rs
│   └── layout.rs    純函式佈局引擎：commit 序列 → 每列 lane/edge/顏色（無 IO）
└── ui/
    ├── mod.rs
    ├── graph_view.rs   graph 線 + commit 列（refs 標籤、訊息、作者、相對時間）
    ├── detail_view.rs  下方詳情面板：完整訊息、hash、作者、日期、檔案列表
    ├── diff_view.rs    全螢幕彩色 diff（新增綠 / 刪除紅、hunk 標頭）
    └── search_bar.rs   / 搜尋輸入列
```

### 邊界原則

- `graph/layout.rs` 是**純函式**：輸入 commit 拓撲資料，輸出佈局結構；不依賴 git2 也不依賴 ratatui。全案風險最高的模組，隔離出來密集單元測試。
- `git/` 層把 git2 型別轉成自家 struct（`types.rs`），git2 型別不外洩到 UI / graph 層。
- `ui/` 只讀 `AppState` 畫畫面，不做業務邏輯。

## 4. Graph 佈局演算法

採 Git Graph / serie 同款的 **lane 分配法**：

1. commit 依 topo + time 順序逐一處理。
2. 維護「進行中的 lanes」清單，每條 lane 記著等待中的 parent oid。
3. 處理 commit C：
   - 有 lane 在等 C → C 落在其中最左的 lane；其餘等 C 的 lanes 是 merge 收攏，畫 `╯` 並關閉。
   - 沒有 lane 等 C → 開新 lane（新 branch tip）。
4. C 的第一個 parent 接手 C 的 lane；其餘 parents（merge 的來源）各開新 lane，畫 `╮` 分叉。
5. 顏色以 lane index 對 8 色盤循環取用。

輸出：`GraphRow { commit_idx, lane, cells: Vec<Cell> }`，`Cell` = 字元（`●` `│` `─` `╮` `╯` `├` `┤`…）+ 顏色。UI 直接照畫，不再做邏輯。

引擎支援**增量計算**：以「目前開放的 lanes」為狀態，新的一塊 commits 接續計算，不重算整張圖。

## 5. 資料流與效能

- 啟動：開 repo → 載入全部 refs → revwalk 分塊載入（**每塊 300 commits**），第一塊到手即渲染；捲動到底自動載下一塊。
- 工作區 dirty 時，graph 頂端插入合成列「● Uncommitted changes (N files)」，選中可看變更檔案（staged + unstaged 合併顯示）。
- commit 的檔案列表與 diff 選中才計算（lazy），並以 LRU（容量 ~50）快取已算過的 commit 詳情。
- `r` 重新載入：全部丟掉重讀（簡單正確優先）。

## 6. 畫面與鍵盤操作

主畫面上下分割：上 70% graph + commit 列表，下 30% 選中 commit 詳情。全螢幕 diff 為獨立 Mode。

```
┌─ my-repo ── main ─────────────────────────────┐
│ ● main  feat: add login page        2h ago    │
│ │ ●  dev  fix: token refresh        5h ago    │
│ ●─┘  merge: dev into main           1d ago    │
│ │ ● feature/ui  style: navbar       2d ago    │
├───────────────────────────────────────────────┤
│ commit a1b2c3d · Warren · 2h ago              │
│ feat: add login page                          │
│  M src/pages/Login.tsx  +120 -8               │
└─ j/k:move  tab:focus  /:search  q:quit ───────┘
```

| 鍵 | 動作 |
|---|---|
| `j/k`、`↑/↓` | 上下移動選取（詳情面板同步更新） |
| `g` / `G` | 跳到頂 / 底 |
| `Tab` | 焦點切換：commit 列表 ↔ 詳情檔案列表 |
| `Enter` | 焦點在檔案列表時：開全螢幕 diff |
| `/` | 增量搜尋（訊息 / 作者 / hash）；`n` / `N` 跳下一個 / 上一個 |
| `b` | branch 篩選選單（只顯示該 branch 可達的 commits；預設全部 refs） |
| `r` | 重新載入 repo |
| `Esc` / `q` | 返回上一層 / 最外層時離開 |

## 7. 錯誤處理

| 情境 | 行為 |
|---|---|
| 不在 git repo / 路徑無效 | stderr 一行友善訊息，exit 1 |
| 空 repo（無 commit） | 顯示「尚無 commit」+ 工作區狀態 |
| diff 遇 binary 檔 | 標示 `binary file`，不輸出內容 |
| diff 遇 rename | 標示 `renamed from <old>` |
| 搜尋範圍 | 只搜已載入 commits；狀態列顯示已搜數量；`n` 到底時自動載入下一塊繼續搜 |
| terminal 異常結束 | panic hook + Drop guard 確保還原 terminal（raw mode / alternate screen） |

## 8. 測試策略

依 `~/.claude/rules/common/testing.md` 新功能標準（新程式碼 80%+ 覆蓋）：

- **graph/layout.rs**：單元測試覆蓋 DAG 形狀 — 線性、單分叉、merge、octopus merge、criss-cross、增量分塊邊界。目標接近 100%。
- **git/**：整合測試用 git2 在 tempdir 建真實 repo fixtures（含 merge、tag、remote ref、dirty worktree）驗證讀取與轉換。
- **ui/**：ratatui `TestBackend` 快照式測試主要畫面（graph 列、詳情、diff、搜尋列）。
- 完成定義：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check` 全綠，並展示輸出。

## 9. 決策紀錄

| 決策 | 選擇 | 落選方案 |
|---|---|---|
| 工具形態 | 互動式 TUI | 靜態輸出 CLI、兩者皆做 |
| 功能範圍 | 純瀏覽（只讀） | 基本操作、lazygit 級完整操作 |
| 技術棧 | Rust + ratatui | Python + Textual、TS + Ink |
| git 存取 + 佈局 | git2 + 自寫 lane 佈局 | 包裝 git CLI 輸出、gitoxide |
| 專案名 | gitgraph-tui | ggv、twig |
