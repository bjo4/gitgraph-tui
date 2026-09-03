# AGENTS.md

Rust TUI，唯讀的 git 歷史圖檢視器。人類貢獻者請直接讀
[CONTRIBUTING.md](CONTRIBUTING.md) —— 本檔只補上 agent 特別容易踩錯的部分。

## 三道閘門

改動後一定要三個都跑過，CI 會擋：

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

動到 `install.sh` 時另外跑：`shellcheck install.sh tests/install.sh && sh tests/install.sh`

## 不可違反的產品不變式

**這個工具永遠不能寫入它所顯示的 repo。** 這是產品承諾，不是內部慣例。
任何會呼叫 git 寫入操作的程式碼都不該存在，包含 `git2` 的
`Repository::set_head`、`checkout_*`、`commit`、`reset`、任何 `_mut` 的寫入路徑。
新增 git2 呼叫時先確認它是唯讀的。

## 三條架構不變式

這三條是 agent 最常在「順手重構」時破壞的：

1. **`git2` 型別不得離開 `src/git/`。** 該模組的公開 API 只回傳 `git/types.rs`
   裡的純資料型別。不要為了方便讓 `Commit<'_>` 洩漏到 `app.rs` 或 `ui/`。
2. **`graph/layout.rs` 必須保持純函式。** 沒有 IO、沒有 git2、沒有 ratatui。
   它把 commit 列表映射成可繪製的格子，並被詳盡地單元測試。
   改動 layout 行為時，要為你改變的 DAG 形狀補一個 glyph-string 測試。
3. **狀態只在 `App::handle_key` 改變。** view 讀狀態，永不改狀態。
   想在 `ui/` 裡加一行 `self.selected += 1` 就是錯的。

## 版本限制

- MSRV 是 **Rust 1.88**，edition 2024。CI 有獨立的 1.88 job。
  用到更新的標準庫 API 前先確認 1.88 有。
- 所有 CI 指令都帶 `--locked`。**不要順手跑 `cargo update`** ——
  升級相依是獨立的 PR，不是附帶改動。

## 測試慣例

- 整合測試用 `tests/common/mod.rs` 的 `Fixture` 建**真實的 git repo**，
  時間戳是決定性的，所以順序斷言穩定。要測 git 行為就用它。
- **不要拿使用者的真實 repo 當測試對象。**
- UI 用 ratatui 的 `TestBackend` 測，範例見 `tests/ui_render.rs` 的 `render_app`。
- 難以透過 git 觸發的錯誤路徑，用注入無效 oid 到 `App.oids` 的方式測，
  範例見 `load_errors_surface_in_status_instead_of_vanishing`。

## commit 訊息

`<type>: <description>`，type 用 feat / fix / refactor / docs / test / chore。
