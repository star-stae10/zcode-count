# AGENTS.md — zcode-count 项目交接说明

> 给后续接手本项目的 AI Agent 的说明文档。读完后你应当能：理解全貌、扩展新功能、修复后续测试出的 bug。
> 本文件是**权威的现状描述**；如与代码不符，以代码为准并顺手更新本文件。

## 1. 这是什么

**zcode-count**：一个 Windows 桌面工具（自用），只读本机 **ZCode**（开源 Agent 工具 `zai-org/ZCode`）的本地用量库，展示**本人**的 token 用量与成本。用于合租 opencode go 套餐时区分各自用量。

- 每人各装一份，只看本机数据（各自机器上的 ZCode 库天然只含本人用量）。
- 技术栈：**Tauri v2 + Rust + React + Vite + TypeScript + Tailwind**（照抄 cc-switch）。
- 仓库：`https://github.com/star-stae10/zcode-count`（public）。

### 数据流（核心）

```
ZCode 用量库  ~/.zcode/cli/db/db.sqlite  (表 model_usage, 只读)
        │  增量 sync（src-tauri/src/zcode/sync.rs）
        ▼
cc-switch 定价库  ~/.cc-switch/cc-switch.db  (表 model_pricing, 只读)
        │  查价 + 成本计算（src-tauri/src/pricing/）
        ▼
自有库  ~/.zcode-count/zcode-count.db  (表 usage_records / sync_cursors / pricing_overrides)
        │  Tauri commands（src-tauri/src/commands.rs）
        ▼
React UI  src/  （汇总卡 + 三页签：请求日志 / Provider 统计 / 模型统计）
```

**自有库是 UI 的唯一真相源**（不直接读 ZCode 库）。好处：① 突破 ZCode 的 30 天保留期（`pruneUsage` 会删旧行），历史长期累积；② ZCode 改 schema 只影响 sync 层。

## 2. 文件地图

```
src-tauri/src/
├─ lib.rs            # 模块注册 + Tauri Builder（setup 注入 OwnDb、invoke_handler 注册 6 个命令）
├─ commands.rs       # AppState、SyncStatus、6 个 Tauri 命令
├─ error.rs          # AppError（thiserror；实现 Serialize 供命令返回）
├─ db/
│  ├─ mod.rs         # OwnDb { pub conn: Mutex<Connection> }、default_db_path()
│  ├─ schema.rs      # migrate()：建 usage_records / sync_cursors / pricing_overrides + 幂等补列
│  └─ dao.rs         # 结构体 + 查询：insert_record/get_cursor/set_cursor/query_summary/
│                    #   query_logs/query_provider_stats/query_model_stats/get_overrides/
│                    #   set_override/query_unpriced_records/update_record_pricing
├─ pricing/
│  ├─ mod.rs         # ModelPricing、cc_switch_db_path()、resolve()
│  ├─ candidates.rs  # model_candidates()：模型名归一化
│  ├─ cost.rs        # calculate_cache_inclusive()：成本计算
│  └─ table.rs       # PricingTable：load(只读) / from_rows / lookup(精确→前缀)
└─ zcode/
   ├─ mod.rs         # zcode_db_path()
   └─ sync.rs        # SyncReport、sync()：增量同步 + 自动重定价

src/
├─ lib/api.ts        # invoke 封装 + 全部前端类型（字段 snake_case，与 Rust Serialize 对齐）
├─ lib/format.ts     # formatTokens/formatCost/formatCostWithUnpriced/formatTime/rangeToWindow/rangeLabel
├─ components/       # Toolbar / SummaryCards / Tabs / RequestLogTable / ProviderStatsTable / ModelStatsTable
└─ App.tsx           # range/tab/status/summary/logs/stats/error/loading state + refresh()

.github/workflows/build.yml   # Windows CI 构建
docs/superpowers/             # 设计文档（specs/）与实施计划（plans/），历史留档
```

## 3. 开发环境（本机 Windows）

- **MSVC 必需**：已装 VS 2022 Build Tools（VCTools + Windows SDK）。Rust 工具链为 `x86_64-pc-windows-msvc`。
- **cargo/rustc 不在默认 PATH**：执行任何 cargo 命令前先
  ```bash
  export PATH="/c/Users/fufu/.cargo/bin:$PATH"
  ```
  （注意 `$USERPROFILE` 是 Windows 反斜杠格式，PATH 里要用 POSIX 路径 `/c/Users/fufu/.cargo/bin`。）
- 前端用 **pnpm**；`node_modules` 在项目目录内。
- 联网走代理（Git Bash 已自动加载 `HTTP_PROXY`），`pnpm`/`cargo`/models.dev 拉取依赖需要它。

### 常用命令

```bash
export PATH="/c/Users/fufu/.cargo/bin:$PATH"
cargo check --manifest-path src-tauri/Cargo.toml   # 必须零 warning
cargo test  --manifest-path src-tauri/Cargo.toml   # Rust 单测
pnpm test                                          # 前端 vitest
pnpm build                                         # tsc + vite
pnpm tauri dev                                     # 本地起桌面应用
pnpm tauri build                                   # 打 Windows 安装包（慢，10-20 分钟）
```

**改动完成前必须**：`cargo check`（零 warning）+ `cargo test` + `pnpm test` + `pnpm build` 全绿，再提交。

## 4. 关键设计决策与坑（务必理解）

1. **成本语义是 cache-inclusive**：ZCode 的 `input_tokens` 是**总输入**（含 cache read/write）。计费输入 = `input - cache_read - cache_creation`，且用 `.max(0)` 钳非负（`pricing/cost.rs`）。实测真实数据满足 `provider_total_tokens = input + output`，cache 从不超 input。**不要**改成 Claude 那种 fresh-input 语义。

2. **同步水位线用「重叠窗口」**（`sync.rs` 的 `SYNC_OVERLAP_MS = 7 天`）：ZCode 的行是**请求完成时**才写入，而 `started_at` 是请求**开始**时间。长请求可能在同步后才入库、其 `started_at < 水位线` → 若只查 `>= 水位线` 会**永久漏计**（真实库有 217 行乱序、最大落后 53 分钟）。所以查询起点是 `last_started_at - 7d`，游标仍存 `max(started_at)`。`INSERT OR IGNORE` 保证重复行被跳过。

3. **mtime 短路**：`db.sqlite` 与 `db.sqlite-wal` 的 mtime 都未变则跳过扫描（纯优化）。**注意**：自动重定价 pass 不受短路影响（定价来源与 ZCode 文件无关）。

4. **幂等**：`request_id` = ZCode `model_usage.id`（主键），`INSERT OR IGNORE`。

5. **自动重定价**：sync 末尾对 `usage_records` 中 `priced = 0` 的行重新查价，命中则更新成本并置 `priced = 1`。这样 cc-switch 补价后历史行会自动回填。

6. **定价来源 = cc-switch 的 `model_pricing` 表（必需依赖）**：
   - 模型名归一化（`pricing/candidates.rs`）：取最后一个 `/` 之后、`:` 之前、转小写、去尾部纯数字后缀。
   - 匹配（`pricing/table.rs`）：先精确，再前缀（`key 以 候选 + "-" 开头`，取最短 key）。
   - **查不到 → `priced = 0`，UI 显示 `—`**；部分未定价显示 `≥ $X`。cc-switch 缺失或表空时成本全部不可用，但 token 统计照常。
   - **重要事实**：ZCode 的 `model_usage` **不存成本**（不像 opencode 存 `cost`）。cc-switch 里 opencode 会话行的成本是它从 opencode 库直接抄的，不是查表算的。所以本工具的成本完全依赖 cc-switch 的定价表覆盖度。

7. **cc-switch 定价覆盖度**：用户须在 cc-switch 里开启 **「自动同步 models.dev 定价」**（使用统计页）并选中用到的模型。cc-switch 的内置种子价是**高峰档**，models.dev 是**空闲档**（如 `deepseek-v4.1-flash` = 0.15/0.6/0.003）；开启同步会覆盖同 ID 内置价。用户主力模型 `deepseek-v4.1-flash` 不在内置种子里，需在 cc-switch「选择模型」里手动勾选。

8. **金额一律 `rust_decimal::Decimal`**，以字符串存 SQLite（列类型 TEXT）。聚合在 Rust 侧用 Decimal 累加（不用 SQL SUM 的 float）。

9. **`query_source`**（ZCode 的 `main_turn`/`subagent`/`session_title`/`compact` 等）全链路贯通到请求日志的「来源」列。

## 5. 数据源 schema 速查

**ZCode `model_usage`**（只读）：`id, provider_id, model_id, query_source, status, started_at(ms), completed_at, duration_ms, time_to_first_token_ms, input_tokens, output_tokens, reasoning_tokens, cache_read_input_tokens, cache_creation_input_tokens, provider_total_tokens, computed_total_tokens, session_id, ...`

**cc-switch `model_pricing`**（只读）：`model_id, display_name, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million`（单价均为「每百万 token 的美元数」，TEXT）。

**自有库 `usage_records`**：`request_id(PK), app_type, provider_id, model_id, query_source, input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd, priced, started_at, duration_ms, first_token_ms, status, session_id, created_at`

**`sync_cursors`**：`source(PK, ZCode库路径), last_started_at, last_mtime, last_synced_at`
**`pricing_overrides`**：`model_id(PK) + 四个单价`（目前无 UI 调用方）

## 6. 扩展新功能的套路

**加一个端到端字段**（最容易漏环，按此顺序）：
1. `db/schema.rs`：`usage_records` 的 `CREATE TABLE` 加列 + `migrate()` 末尾加**幂等补列**（`PRAGMA table_info` 检查后 `ALTER TABLE ADD COLUMN`）。
2. `db/dao.rs`：`UsageRecord` 加字段 → `insert_record` 的 SQL 列/占位符/`params!` **顺序严格对齐** → 相关查询结构体（如 `RequestLogRow`）加字段 + SELECT + `row.get(n)` **索引对齐**。
3. `zcode/sync.rs`：`ZRow` 加字段 → `query_rows` 的 SELECT + `row.get(n)` → 构造 `UsageRecord` 时带上。
4. `src/lib/api.ts`：对应 interface 加字段（snake_case）。
5. 前端组件渲染。
6. 测试：Rust（dao/sync）+ 前端组件测试。
7. 验证四件套。

**加一个页签**：`components/Tabs.tsx` 加 tab → 新建表格组件 → `App.tsx` 加 state + refresh 里拉数据 + 分支渲染。

**加一个命令**：`commands.rs` 写 `#[tauri::command]` → `lib.rs` 的 `generate_handler!` 注册 → `api.ts` 封装。
> 注意 Tauri v2 参数默认按 camelCase 映射到 Rust 的 snake_case；多词参数（如 `model_id`）前端要传 `modelId`，或给命令加 `rename_all = "snake_case"`。

**测试风格**：Rust 用文件内 `#[cfg(test)]`（内存 SQLite / 临时文件）；前端用 vitest + `renderToStaticMarkup`（无 jsdom）。测试要**真断言行为**，别只断言「不 panic」。

## 7. 发布流程

```bash
git add -A && git commit -m "..."
git push origin master
git tag vX.Y.Z && git push origin vX.Y.Z     # 触发 CI
gh run list / gh run watch                    # 查看构建
gh run download <run-id> -D out               # 下载 artifact（含 .msi 与 setup.exe）
```
CI：`.github/workflows/build.yml`，`windows-latest`，跑 `cargo test` + `pnpm test` + `pnpm tauri build`，上传 artifact `zcode-count-windows`。

## 8. 已知限制 / 待办（可改进项）

- **模型名归一化对 `:` 处理过激**：`gemma4:31b-cloud` 会被截成 `gemma4` → 查不到价（显示 `—`）。如需支持 ollama 风格 ID 要改 `pricing/candidates.rs`。
- **定价候选是 cc-switch 的简化子集**：未实现 ISO 日期变体、reasoning-effort 后缀、claude `.`→`-`、前缀匹配门控等。换模型后可能漏配/错配（实测当前数据无错配）。
- **前端 `refresh()` 竞态**：快速切换时间范围时无请求序号，旧响应可能覆盖新状态；且 `refresh` 串行 5 个 IPC await，`sync` 失败会阻断数据刷新。
- **切换时间范围会触发一次 sync**（mtime 短路使开销很小，但语义耦合）。
- **死字段**：`SyncStatus.last_error` 恒为 None；`usage_records.created_at` 实际存的是 `started_at`。
- **重定价触发条件过窄**：仅当 `pricing` 表非空才执行（`pricing_overrides` 单独存在时不触发）。
- **模板残留**：`index.html` 标题/favicon 仍是脚手架值；`Cargo.toml` 的 `description`/`authors` 是默认值；`@tauri-apps/plugin-opener` 依赖未使用。
- **迁移无 `PRAGMA user_version`**：靠 ad-hoc 列检查。
- **`formatCost` 对极小金额显示 `$0.0000`**。
- **CI**：actions 有 Node 20 deprecation 警告（不影响构建）；可考虑升级 action 版本。

## 9. 历史留档

- 设计文档：`docs/superpowers/specs/2026-09-23-zcode-count-design.md`
- 实施计划（9 个任务 + 2 个补充任务 + 最终修复波）：`docs/superpowers/plans/2026-09-23-zcode-count.md`
- 实施过程用了 subagent-driven-development：每任务独立实现 + 代码审查；最终整分支审查发现并修复了 1 个 Critical（水位线永久漏行）。过程中每个任务都有 commit，可用 `git log --oneline` 追溯。

## 10. 成本误差参考

用户实测：成本统计误差约 **0.0063%**，远低于其可接受的 1.5% 阈值。改动成本链路后，请用真实数据回归确认误差仍在阈值内（可与 cc-switch 的用量页对比同期数据）。
