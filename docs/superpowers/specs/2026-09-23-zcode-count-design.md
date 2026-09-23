# zcode-count 设计文档

- 日期：2026-09-23
- 状态：已与用户确认
- 技术栈：Tauri v2 + React + Vite + TypeScript + Tailwind + Rust（照抄 cc-switch）

## 1. 背景与目标

用户与朋友**合租 opencode go 套餐**，共用同一账号 / API key，各自电脑各自安装 ZCode。官方面板只能看到账号总量，无法区分每个人的用量。

目标：做一个**自用桌面工具 zcode-count**，读取本机 ZCode 的本地用量库，展示**本机（即本人）**的 token 用量、请求数与成本。每人各装一份、各看各的，即可天然区分各自用量。

### 非目标（YAGNI）

- 不做 HTTP 代理拦截（ZCode 自己已把用量写入本地库，无需拦流）。
- 不读其它 agent（Claude Code / Codex / OpenCode）——本工具只针对 ZCode。
- 不做多机数据合并 / 云同步。
- 不做 models.dev 在线定价同步（定价由 cc-switch 维护）。
- 仅面向 Windows。

## 2. 术语

- **ZCode**：开源 Agent 工具（`zai-org/ZCode`），本地会话与用量存于 SQLite。
- **cc-switch**：MIT 协议的开源工具，其 `model_pricing` 表作为本工具的定价来源（**必需依赖**）。
- **自有库**：zcode-count 自己的 SQLite 库，作为 UI 的唯一真相源。

## 3. 数据源

| 角色 | 路径（Windows） | 访问方式 |
|---|---|---|
| ZCode 用量库 | `%USERPROFILE%\.zcode\cli\db\db.sqlite` | 只读 |
| cc-switch 定价库 | `%USERPROFILE%\.cc-switch\cc-switch.db` | 只读 |
| zcode-count 自有库 | `%USERPROFILE%\.zcode-count\zcode-count.db` | 读写 |

### ZCode 用量库

主表 `model_usage`（每次模型请求一行），关键字段：

- 标识：`id`（唯一主键）、`session_id`、`turn_id`、`trace_id`
- 归因：`provider_id`、`model_id`、`query_source`、`status`
- 时间：`started_at`（毫秒）、`completed_at`、`duration_ms`、`time_to_first_token_ms`
- token：`input_tokens`、`output_tokens`、`reasoning_tokens`、`cache_read_input_tokens`、`cache_creation_input_tokens`、`provider_total_tokens`、`computed_total_tokens`

其它表（`session`、`turn_usage`、`tool_usage`）MVP 不读取；会话数由自有库 `usage_records` 中 `distinct session_id` 计算。

**注意**：ZCode 的 `model_usage` **不含成本字段**，成本需自行计算。

### cc-switch 定价库

表 `model_pricing`：

- `model_id`（主键）、`input_cost_per_million`、`output_cost_per_million`、`cache_read_cost_per_million`、`cache_creation_cost_per_million`

### 自有库 schema（新建）

- `usage_records`：从 ZCode `model_usage` 增量同步来的明细。主键 `request_id`（= ZCode `model_usage.id`），含 `app_type='zcode'`、`provider_id`、`model_id`、四类 token、四类成本、`total_cost_usd`、`started_at`、`duration_ms`、`first_token_ms`、`status`、`session_id`。
- `sync_cursors`：增量同步水位线（按 ZCode DB 文件路径记录 `last_started_at`）。
- `pricing_overrides`：应用内手动补价（可选，用于覆盖 cc-switch 缺失的模型）。

## 4. 成本计算

ZCode 的 `input_tokens` 是 **cache-inclusive**（总输入，含 cache read/write）。实测样例：`input=53292, cache_read=46464, output=2805, total=56097=input+output`。

计费公式（Decimal 高精度，避免浮点误差）：

```
billable_input = input_tokens - cache_read_input_tokens - cache_creation_input_tokens   (saturating_sub)
input_cost        = billable_input       * input_price        / 1_000_000
output_cost       = output_tokens        * output_price       / 1_000_000
cache_read_cost   = cache_read_tokens    * cache_read_price   / 1_000_000
cache_creation_cost = cache_creation_tokens * cache_creation_price / 1_000_000
total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost
```

说明：`output_tokens` 已包含 `reasoning_tokens`（ZCode 侧语义），不再叠加。

### 模型名 → 定价解析

复用 cc-switch 的解析语义：先做模型名归一化（去 namespace 前缀、去日期/版本后缀等）生成候选，再精确匹配，最后按前缀匹配（`model_id LIKE '候选-%' ORDER BY LENGTH ASC LIMIT 1`）。

查不到定价的行：成本记 `0`，UI 成本列显示"—"；可在应用内手动补价（写入自有库 `pricing_overrides`，不修改 cc-switch 库）。

## 5. 架构与数据流

```
ZCode DB ──(增量 sync)──▶ zcode-count DB ──(Tauri commands)──▶ React UI
cc-switch 定价 ──(查价)──┘        ▲
                             水位线 / 历史 / 汇总 / 价格覆盖
```

**单一真相源 = 自有库**：UI 只查自有库，不直接读 ZCode DB。好处：

1. 突破 ZCode 的 30 天保留期（`pruneUsage` 删除 30 天前的 `model_usage` 行），历史长期累积。
2. ZCode 改 schema 时只影响 sync 层。

**同步流程**（打开应用自动执行一次，或点「刷新」触发）：

1. 读自有库中该 ZCode DB 路径的水位线 `last_started_at`。
2. 校验 ZCode DB 与 `-wal` 的 mtime；都未变化则跳过。
3. 只读打开 ZCode DB，查 `model_usage WHERE started_at > 水位线`。
4. 逐行查价、算成本，`INSERT OR IGNORE` 入自有库 `usage_records`。
5. 全部成功才推进水位线；任一行出错则不推进，下次重试（幂等靠 `request_id` 主键）。

## 6. 功能与 UI

**顶部汇总卡**：所有模型合计——总 token（输入 / 输出 / 缓存读 / 缓存写 / 推理）、总请求数、会话数、总成本、时间范围。

**三个页签**：

| 页签 | 内容 |
|---|---|
| 请求日志 | 表格：时间 / 供应商 / 计费模型 / 输入（下方小字显示 cache read）/ 输出 / 总成本 / 用时·首字 / 状态 / 来源。筛选：供应商、时间范围（全部 / 当天 / 7 天 / 30 天 / 自定义） |
| Provider 统计 | 按 `provider_id` 聚合：请求数、输入 / 输出 token、总成本 |
| 模型统计 | 先给「所有模型」汇总行，再按 `model_id` 逐行：请求数、输入 / 输出 token、总成本；支持按成本 / token 排序 |

**交互**：打开应用自动 sync 一次；顶部「刷新」按钮；显示「上次同步时间」。

**状态列**：直接映射 ZCode 的 `completed` / `error` / `cancelled`。

## 7. 工程结构

```
zcode-count/
├─ src/                      # React 前端：汇总卡 + 三页签 + 表格
│  ├─ components/  lib/  App.tsx  main.tsx
├─ src-tauri/
│  ├─ src/
│  │  ├─ db/         # 自有库 schema + DAO（usage_records / sync_cursors / pricing_overrides）
│  │  ├─ zcode/      # 只读读 ZCode DB，增量 sync
│  │  ├─ pricing/    # 读 cc-switch 定价 + 模型名候选/前缀解析
│  │  ├─ commands.rs # Tauri commands（sync / summary / logs / stats）
│  │  └─ lib.rs
│  ├─ Cargo.toml  tauri.conf.json
├─ .github/workflows/build.yml
├─ package.json  README.md
```

### 依赖

- Rust：`tauri` v2、`rusqlite`(bundled)、`serde` / `serde_json`、`rust_decimal`、`chrono`、`thiserror`
- 前端：`react`、`vite`、`typescript`、`tailwindcss`、`@tauri-apps/api`
- 包管理 pnpm；`node_modules` 置于项目目录内（遵循开发目录约定）；需要全局安装的工具放 `D:\dev`

### 应用标识

- `identifier`：`com.fufu.zcode-count`
- `productName`：`zcode-count`

## 8. CI 与交付

### GitHub Actions（`.github/workflows/build.yml`）

- 触发：打 `v*` tag，或手动 `workflow_dispatch`
- Runner：`windows-latest`
- 步骤：checkout → 装 Node/pnpm 与 Rust → `pnpm install` → `pnpm tauri build` → 上传 `.msi` 与 `setup.exe` 为 artifact

### GitHub 交付流程

1. `git init` + 初始提交（含本设计文档）
2. `gh repo create zcode-count --public --source . --push`
3. 确认 Actions 跑通
4. 从 artifact 下载安装包

## 9. 错误处理与健壮性

- ZCode DB 缺失 / 被占用 → 友好提示、可重试
- cc-switch DB 缺失或 `model_pricing` 为空 → 成本显示"—"并提示「请安装 cc-switch 以获取定价」，token 统计照常可用
- WAL：同时检查 `db.sqlite` 与 `db.sqlite-wal` 的 mtime
- 只读方式打开外部 DB，避免与正在运行的 ZCode / cc-switch 抢锁
- sync 出错不推进水位线，保证下次重试；幂等由 `request_id` 主键保证

## 10. 测试与验收

- Rust 单测：
  - 成本计算（cache-inclusive 语义、各桶单价、Decimal 精度）
  - 模型名 → 定价解析（精确 / 前缀 / 归一化候选）
  - sync 幂等与水位线推进（内存 SQLite）
- 人工校验：把 zcode-count 的总 token 与 ZCode 自带「App Usage」页对一遍

## 11. 关键决策记录

| 决策 | 结论 | 理由 |
|---|---|---|
| 数据获取方式 | 只读 ZCode 本地 DB，不做代理拦截 | ZCode 已自行落库，最简单可靠 |
| 真相源 | 自有库 | 突破 30 天保留期、解耦 schema 变更 |
| 定价来源 | 复用 cc-switch 的 `model_pricing`（**必需依赖**） | 用户不愿自行维护价格表，让 cc-switch 一劳永逸维护 |
| 交付范围 | 仅 Windows | 双方均为 Windows，CI 最简 |
| 技术方案 | 全新精简 Tauri 应用 | 聚焦、体积小、可维护，便于日后扩展 |
