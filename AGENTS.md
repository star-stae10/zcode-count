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
├─ lib.rs            # 模块注册 + Tauri Builder（setup 注入 OwnDb、invoke_handler 注册 15 个命令）
├─ commands.rs       # AppState、SyncStatus、14 个 Tauri 命令（统计 4 + 导出 1 + 清除/审计 4 + 未定价/层级 2 + 定价覆盖 3）
├─ export.rs         # 请求日志导出渲染（CSV/JSON）：字段与请求日志 10 列一致，CSV 带 BOM/CRLF/引号转义，JSON 英文键 + 机器值
├─ error.rs          # AppError（thiserror；实现 Serialize 供命令返回）
├─ db/
│  ├─ mod.rs         # OwnDb { pub conn: Mutex<Connection> }、default_db_path()
│  ├─ schema.rs      # migrate()：usage_records / sync_cursors / pricing_overrides / audit_log / clear_tombstones + 幂等补列
│  └─ dao.rs         # 结构体 + 查询 + ScopeFilter 范围筛选（scope_condition/time_condition，统计 4 查询共用）
│                    #   + 清除（validate_clear_scope/preview_clear/clear_records，事务：删除+墓碑+游标+审计）
│                    #   + 审计（insert_audit_log/list_audit_logs）+ 墓碑（list_tombstones/is_tombstoned）
│                    #   + 未定价清单（query_unpriced_models，含成本估算区间）+ 层级（list_provider_models）
├─ pricing/
│  ├─ mod.rs         # ModelPricing、TieredPricing{off_peak,peak}+pick()、cc_switch_db_path()、override_lookup()、resolve()（返回 TieredPricing）
│  ├─ candidates.rs  # model_candidates()：模型名归一化（全仓库唯一归一化规则源）
│  ├─ tier.rs        # is_peak(started_at_ms)：DeepSeek 峰谷判档纯函数（UTC+8、周一至五 9-12/14-18、节假日内置表 2026-09-01~2027-01-31；不引入 chrono）
│  ├─ cost.rs        # calculate_cache_inclusive()：成本计算
│  └─ table.rs       # PricingTable：load(只读) / from_rows / lookup(精确→前缀)
└─ zcode/
   ├─ mod.rs         # zcode_db_path()
   ├─ provider_names.rs # 从 ZCode provider_config.json 读 (providerId → providerName/baseUrl)；缺失→空、损坏→Err、缺字段→跳过
   └─ sync.rs        # SyncReport（含 tombstoned 计数）、sync()：增量同步 + 墓碑过滤 + 自动重定价 + 名称快照 upsert（在 mtime 短路之前）

src/
├─ lib/api.ts        # invoke 封装 + 全部前端类型（字段 snake_case，与 Rust Serialize 对齐）
├─ lib/format.ts     # formatTokens/formatCost/formatCostWithUnpriced/formatTime/rangeToWindow/rangeLabel
│                    #   + costConfidence（成本可信度）+ formatDateTime + dateStart/dateEndExclusive
│                    #   + exportFileName（导出默认文件名，时间范围编入文件名）
├─ lib/providerName.ts # providerLabel(names, id)：供应商显示名映射（无映射回退原始 ID）；显示用名称、传参/过滤一律仍用原始 provider_id
├─ components/       # Toolbar / SummaryCards / Tabs / RequestLogTable / ProviderStatsTable / ModelStatsTable
│                    #   / PricingOverrideDialog（支持预填 + 命中数/重算反馈）/ ScopePicker（供应商>模型层级多选，核算与清除共用）
│                    #   / ScopeDialog（核算范围）/ ClearDataDialog（清除流程）/ UnpricedDialog / AuditLogDialog
│                    #   ——凡显示供应商处均接 names 映射（title 保留原始 ID）
└─ App.tsx           # range/scope/tab/status/summary/logs/stats/unpriced state + refresh()（Promise.all 并行 + 请求序号防竞态）

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

5. **自动重定价**（sync 末尾两段，均不受 mtime 短路影响）：
   - (a) **覆盖重算**：对每个 `pricing_overrides` 的 `(provider, model)`，重算该组合的**所有**行（含已定价），计入 `repriced`。
   - (b) **表回填**：对 `usage_records` 中 `priced = 0` 的行重新查价（`resolve`），命中则更新并置 `priced = 1`。cc-switch 补价后历史行自动回填。

6. **定价来源 = cc-switch 的 `model_pricing` 表（必需依赖）**：
   - 模型名归一化（`pricing/candidates.rs`）：取最后一个 `/` 之后、`:` 之前、转小写、去尾部纯数字后缀。
   - 匹配（`pricing/table.rs`）：先精确，再前缀（`key 以 候选 + "-" 开头`，取最短 key）。
   - **查不到 → `priced = 0`，UI 显示 `—`**；部分未定价显示 `≥ $X`。cc-switch 缺失或表空时成本全部不可用，但 token 统计照常。
   - **重要事实**：ZCode 的 `model_usage` **不存成本**（不像 opencode 存 `cost`）。cc-switch 里 opencode 会话行的成本是它从 opencode 库直接抄的，不是查表算的。所以本工具的成本完全依赖 cc-switch 的定价表覆盖度。

7. **cc-switch 定价覆盖度**：用户须在 cc-switch 里开启 **「自动同步 models.dev 定价」**（使用统计页）并选中用到的模型。cc-switch 的内置种子价是**高峰档**，models.dev 是**空闲档**（如 `deepseek-v4.1-flash` = 0.15/0.6/0.003）；开启同步会覆盖同 ID 内置价。用户主力模型 `deepseek-v4.1-flash` 不在内置种子里，需在 cc-switch「选择模型」里手动勾选。

8. **金额一律 `rust_decimal::Decimal`**，以字符串存 SQLite（列类型 TEXT）。聚合在 Rust 侧用 Decimal 累加（不用 SQL SUM 的 float）。

9. **`query_source`**（ZCode 的 `main_turn`/`subagent`/`session_title`/`compact` 等）全链路贯通到请求日志的「来源」列。

10. **核算范围筛选全局生效（供应商 > 模型层级）**：原「供应商单选下拉」已升级为「核算范围」多选（`ScopeFilter { providers: Vec<String>, models: Vec<ModelSel> }`，供应商级与模型级**并集**生效）。传给 summary / 请求日志 / Provider 统计 / 模型统计 / 未定价清单五类查询（SQL 片段由 `dao::scope_condition` 动态生成，`provider_id IN (...) OR (provider_id=? AND model_id=?)`）。空 scope = 不筛选（全部）。模型统计页「所有模型」合计行取自筛选后的 summary，与明细一致。核算范围弹窗与定价覆盖弹窗的供应商**选项**始终来自未筛选的全量统计（`App.tsx` 的 `providerOptions`）。**核算范围只影响统计与展示，绝不删除数据**——与「清除数据」在 UI（蓝 vs 红）、文案上严格区分。

11. **清除 = 删除 + 审计 + 墓碑**（`dao::clear_records`，单个事务）：
    - 范围 = 时间段（可选）+ 供应商集合 + `(provider, model)` 对集合，并集语义；全空 = 全清。
    - 流程：`validate_clear_scope`（时间倒置 / 供应商不存在 / 模型不存在或归属不符，均明确报错）→ 计数（=0 报「所选范围内没有数据」）→ **授权确认短语校验**（`删除N`，N 为预览条数；预览后数据变化会自然校验失败，防误删）→ DELETE → 写 `clear_tombstones` → 全清时重置 `sync_cursors` → 写 `audit_log`。
    - **墓碑是关键**：ZCode 源库只读，sync 重叠窗口（7 天）会把已删行从源库重新灌回。sync 扫描后、插入前用 `dao::is_tombstoned` 过滤（命中计入 `SyncReport.tombstoned`）。墓碑时间边界缺省 `[0, 清除时刻]`；全清写一条全域墓碑并重置游标。清除后新请求（`started_at` 晚于清除时刻）正常进入；清除时刻前已开始、之后才完成的行会被挡（属"清除时已存在的历史"，语义如此）。若要恢复，需手动删墓碑行 + 重置游标（无 UI）。
    - 权限：单机工具无账号体系，「仅授权用户」由后端强制校验确认短语实现；审计「操作人」取 `USERNAME` 环境变量。
    - 预览命令 `preview_clear_usage`（dry-run）返回条数与 `confirm_token`；清除命令 `clear_usage` 再算一遍条数并校验短语。

12. **未定价警告与成本可信度**：`Summary` 含 `unpriced_tokens`（未定价行 input+output）与 `unpriced_models`（未定价 (provider, model) 组合数）。`unpriced_count > 0` 时 SummaryCards 显示琥珀色警告（文案明示"实际成本可能高于显示值"）+ 可信度（`format.ts::costConfidence`：未定价 token 占比 0% → 高 / <20% → 中 / 其余 → 低）。`UnpricedDialog` 明细来自 `get_unpriced_models`：按 (provider, model) 分组，估算区间 = 未定价 token × 同供应商已定价行的每 token 单价范围（`[min,max]`），无可参照行 → None（显示"无法估算"）。每行有「补充定价」按钮 → 打开 `PricingOverrideDialog` 并预填该组合。

13. **供应商名称映射（展示层）**：`provider_names` 表在每次 sync 时从 ZCode `provider_config.json` 快照 upsert（**放在 mtime 短路之前**——provider 改名不改用量库；只增不删，provider 删除后历史数据仍可读；`source='manual'` 语义预留、sync 不覆盖它）。展示层一律 `providerLabel()` 显示名称、`title`/传参/过滤保留原始 `provider_id`；无映射回退原始 ID，配置缺失/损坏不阻断 sync 与刷新。统计与传参口径零改动。

14. **覆盖匹配口径统一（单一规则源）**：`pricing::override_lookup` = 先 `(provider, model)` 精确、再按 `model_candidates` 归一化候选匹配，provider 恒精确、空候选覆盖跳过、精确优先。`resolve()` 内部改调它（签名不变）；sync 覆盖重算 pass（`list_models_for_provider` + `override_lookup` 判定命中）与删除覆盖清理（`delete_override_and_clear`）同口径。**禁止在别处另写匹配逻辑**——历史教训：覆盖曾用精确匹配而表价用归一化，同一规则被复制三份导致覆盖静默失效。`OverrideRow.matched_count` 也必须复用 `override_lookup` 计算。

15. **DeepSeek 峰谷定价（以定价覆盖为载体，已获所有者确认实施）**：`pricing_overrides` 可选带 4 个 peak 单价列（NULL = 未启用峰谷）；启用组合由 `TieredPricing::pick(started_at)` 按行判档选价，历史由覆盖重算 pass 全量回填；判档只在 `pricing/tier.rs` 一处（前端「计费档」列由 `list_logs` 命令后处理标注，前端零判档逻辑）。峰/谷两组价全部由用户手填（不依赖 cc-switch 表价口径）；`set_price_override` 的 peak 四参数全填=启用、全空=关闭（普通覆盖价保留）、部分填=报错。节假日表仅覆盖 2026-09-01~2027-01-31（国办发明电〔2025〕7号），**2027-02 起按周几退化，需手动维护 tier.rs**。真实数据对账：双实现互验差异 0.0000%，峰谷较旧单一空闲档 +10.59%（本机窗口）。

16. **导出请求日志（CSV/JSON）**：工具栏「导出」按钮 → `tauri-plugin-dialog` 保存对话框选路径（capabilities 需 `dialog:default`）→ `export_logs` 命令写盘。**口径与 `list_logs` 完全一致**（同 `query_logs` + `annotate_price_tiers` + 供应商显示名映射），但 `limit = i64::MAX` 不截断——导出当前时间范围 + 核算范围下的**全部**行（页签为 500 条）。文件内容渲染集中在 `export.rs`：CSV 中文列头与档位（峰/谷）、UTF-8 BOM + CRLF（Excel 直开不乱码）、引号转义；JSON 英文键（time/provider/model/input_tokens/output_tokens/total_cost_usd/price_tier/duration_text/status/source）、RFC 3339 本地时间、未定价成本为 null（对应 UI「—」）、机器值 "peak"/"off_peak"。修改导出字段时同步改 `csv_fields` 与 `ExportRow` 两处 + `export.rs` 单测。

## 5. 数据源 schema 速查

**ZCode `model_usage`**（只读）：`id, provider_id, model_id, query_source, status, started_at(ms), completed_at, duration_ms, time_to_first_token_ms, input_tokens, output_tokens, reasoning_tokens, cache_read_input_tokens, cache_creation_input_tokens, provider_total_tokens, computed_total_tokens, session_id, ...`

**cc-switch `model_pricing`**（只读）：`model_id, display_name, input_cost_per_million, output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million`（单价均为「每百万 token 的美元数」，TEXT）。

**自有库 `usage_records`**：`request_id(PK), app_type, provider_id, model_id, query_source, input_tokens, output_tokens, reasoning_tokens, cache_read_tokens, cache_creation_tokens, input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd, priced, started_at, duration_ms, first_token_ms, status, session_id, created_at`

**`sync_cursors`**：`source(PK, ZCode库路径), last_started_at, last_mtime, last_synced_at`
**`pricing_overrides`**：`(provider_id, model_id)(PK) + 四个单价 + 四个峰段单价(peak_*，NULL=未启用峰谷)`（UI：工具栏「定价覆盖」弹窗，「适配 DeepSeek 峰谷定价」按钮）
**`provider_names`**：`provider_id(PK), display_name, source('zcode_config'|'manual'), base_url, first_seen_at, updated_at`（sync 快照 upsert，只增不删）
**`audit_log`**：`id, actor(操作人), action('clear_usage'), started_at_from/to(时间段,可 NULL), providers(JSON 数组), models(JSON 数组 [{provider_id,model_id}]), deleted_count, created_at`
**`clear_tombstones`**：`id, ts_from, ts_to(闭区间,ms), provider_id(NULL=不限), model_id(NULL=该供应商下不限), created_at`

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

## 7. 发布流程（v1.0.0 起：tag → CI 自动出 GitHub Release）

**机制**：`.github/workflows/build.yml` 在 `windows-latest` 上跑 `cargo test` + `pnpm test` + `pnpm tauri build`；构建成功后由 `softprops/action-gh-release@v2` 步骤（仅当触发 ref 为 `v*` tag 时执行，手动 workflow_dispatch 不发）**自动创建 GitHub Release 并上传安装包资产**（NSIS `*-setup.exe` + MSI `.msi`），public 仓库资产匿名可下载。Release 产生的是「资产」，与仅供登录用户下载的 Actions artifact 是两回事—— Releases 页可点击下载的就是资产。

**发版步骤（AI 可全程代执行）**：

```bash
# 1. 版本号三处同步（缺一不可）：package.json、src-tauri/Cargo.toml、src-tauri/tauri.conf.json
#    （改 Cargo.toml 后跑一次 cargo check 让 Cargo.lock 随动）
# 2. 更新 CHANGELOG.md：新增「## [X.Y.Z] - 日期」段（新增/修复/其他 + 下载说明，参照 v1.0.0 段格式）
# 3. 提交并打 tag：
git add -A && git commit -m "chore(release): vX.Y.Z"
git push origin master
git tag vX.Y.Z && git push origin vX.Y.Z        # 推 tag 即触发 CI 构建并自动发 Release
# 4. 监视与验证：
gh run watch <run-id>                            # 约 5-10 分钟，确认「Create GitHub Release and upload installers」步骤 ✓
gh release view vX.Y.Z                           # 核对资产（.exe + .msi 各 1 个）
gh api repos/star-stae10/zcode-count/releases/latest --jq .tag_name   # 确认 latest 指向新版本
```

**Release notes 维护**：CI 默认生成 commit 列表当说明；发版后用 CHANGELOG 对应段覆盖：
`sed -n '/^## \[X.Y.Z\]/,/^## \[X.Y.W-1\]/p' CHANGELOG.md | sed '$d' > notes.md && gh release edit vX.Y.Z --title "vX.Y.Z" --notes-file notes.md`

**注意事项**：
- tag 必须打在包含最新版本号三处改动的 commit 上，否则二进制内版本与 tag 不一致（只影响显示，无自动更新机制，但仍应保持一致）。
- 发版前四件套必须全绿（§3）。
- 不打 tag 就不会有任何 Release 动作；发错版本可在 GitHub Release 页删除 Release 与 tag 重来（安装包资产会被 CI 重新上传）。

## 8. 已知限制 / 待办（可改进项）

- **模型名归一化对 `:` 处理过激**：`gemma4:31b-cloud` 会被截成 `gemma4` → 查不到价（显示 `—`）。如需支持 ollama 风格 ID 要改 `pricing/candidates.rs`。
- **定价候选是 cc-switch 的简化子集**：未实现 ISO 日期变体、reasoning-effort 后缀、claude `.`→`-`、前缀匹配门控等。换模型后可能漏配/错配（实测当前数据无错配）。
- **切换时间范围会触发一次 sync**（mtime 短路使开销很小，但语义耦合）。
- **死字段**：`SyncStatus.last_error` 恒为 None；`usage_records.created_at` 实际存的是 `started_at`。
- **模板残留**：`Cargo.toml` 的 `description`/`authors` 是默认值；`@tauri-apps/plugin-opener` 依赖未使用。（index.html 标题/favicon 已随图标更换修复。）
- **应用图标**：源图为 `src-tauri/icons/source.png`（1024×1024 RGBA，黑底方形设计，**不要抠透明**——发光效果依赖黑底）。重新生成全套图标用 `pnpm tauri icon src-tauri/icons/source.png`，生成后会附带 android/ios 目录，桌面项目用不到可删除。
- **迁移无 `PRAGMA user_version`**：靠 ad-hoc 列检查。
- **`formatCost` 对极小金额显示 `$0.00000`**（5 位小数）。
- **CI**：actions 有 Node 20 deprecation 警告（不影响构建）；可考虑升级 action 版本。
- **峰谷节假日表需手动维护**：`pricing/tier.rs` 的 `HOLIDAY_DAYS` 仅覆盖 2026-09-01~2027-01-31（所有者确认范围），2027-02 起按周几退化判定（周末节假日会被当工作日峰段计价）；跨年后须按国办最新安排补表。

## 9. 历史留档

- 设计文档：`docs/superpowers/specs/2026-09-23-zcode-count-design.md`
- 实施计划（9 个任务 + 2 个补充任务 + 最终修复波）：`docs/superpowers/plans/2026-09-23-zcode-count.md`
- 实施过程用了 subagent-driven-development：每任务独立实现 + 代码审查；最终整分支审查发现并修复了 1 个 Critical（水位线永久漏行）。过程中每个任务都有 commit，可用 `git log --oneline` 追溯。

## 10. 成本误差参考

用户实测：成本统计误差约 **0.0063%**，远低于其可接受的 1.5% 阈值。改动成本链路后，请用真实数据回归确认误差仍在阈值内（可与 cc-switch 的用量页对比同期数据）。
