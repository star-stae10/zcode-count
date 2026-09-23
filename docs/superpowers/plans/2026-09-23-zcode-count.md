# zcode-count Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个 Windows 桌面工具 zcode-count，只读本机 ZCode 的 SQLite 用量库，展示本人的 token 用量、请求数与成本。

**Architecture:** Tauri v2 应用。Rust 后端把 ZCode 的 `model_usage` 增量同步进自有库 `~/.zcode-count/zcode-count.db`，同步时用 cc-switch 的 `model_pricing` 定价算成本；前端只查自有库，渲染汇总卡 + 三个页签（请求日志 / Provider 统计 / 模型统计）。

**Tech Stack:** Tauri v2、Rust（rusqlite bundled、rust_decimal、chrono、serde、thiserror）、React + Vite + TypeScript + Tailwind、pnpm。

## Global Constraints

- 仅面向 Windows；Tauri bundle targets = `["msi", "nsis"]`。
- `identifier` = `com.fufu.zcode-count`；`productName` = `zcode-count`。
- 外部数据源路径：ZCode = `%USERPROFILE%\.zcode\cli\db\db.sqlite`（只读）；cc-switch = `%USERPROFILE%\.cc-switch\cc-switch.db`（只读）；自有库 = `%USERPROFILE%\.zcode-count\zcode-count.db`（读写）。
- **cc-switch 是必需依赖**（提供定价）。找不到 cc-switch 库或 `model_pricing` 为空时，成本列显示 `—`，token 统计照常可用。
- ZCode 的 `input_tokens` 是 **cache-inclusive**（总输入，含 cache read/write）；计费输入 = `input - cache_read - cache_creation`（saturating）。
- 不拦截 HTTP 代理；只读 ZCode 的 `model_usage` 表。
- 包管理用 pnpm；`node_modules` 置于项目目录内。仓库路径：`E:\opencode learning\05\zcode-count`。
- 金额一律用 `rust_decimal::Decimal`，以字符串存 SQLite。
- 所有外部 DB 以只读方式打开（`SQLITE_OPEN_READ_ONLY`）。

---

## File Structure

```
zcode-count/
├─ package.json / pnpm-lock.yaml / vite.config.ts / tsconfig.json / tsconfig.node.json
├─ tailwind.config.cjs / postcss.config.cjs / index.html
├─ src/
│  ├─ main.tsx / App.tsx / index.css
│  ├─ lib/api.ts            # Tauri invoke 封装 + 共享类型
│  ├─ lib/format.ts         # token / 金额 / 时间格式化
│  ├─ lib/format.test.ts    # vitest
│  └─ components/
│     ├─ Toolbar.tsx        # 刷新 + 上次同步 + cc-switch 缺失提示
│     ├─ SummaryCards.tsx
│     ├─ Tabs.tsx
│     ├─ RequestLogTable.tsx
│     ├─ ProviderStatsTable.tsx
│     └─ ModelStatsTable.tsx
├─ src-tauri/
│  ├─ Cargo.toml / build.rs / tauri.conf.json
│  ├─ capabilities/default.json
│  ├─ icons/...
│  └─ src/
│     ├─ main.rs / lib.rs / error.rs / commands.rs
│     ├─ db/{mod.rs, schema.rs, dao.rs}
│     ├─ pricing/{mod.rs, table.rs, candidates.rs, cost.rs}
│     └─ zcode/{mod.rs, sync.rs}
├─ .github/workflows/build.yml
├─ docs/superpowers/...
└─ README.md
```

---

### Task 1: 脚手架与构建打通

**Files:**
- Create（由脚手架生成后调整）: 项目根配置、`src/*`、`src-tauri/*`

**Interfaces:**
- Consumes: 无
- Produces: 可 `pnpm build` 的前端、可 `cargo check` 的后端、`identifier`/`productName`/bundle targets 已就位。

- [ ] **Step 1: 生成脚手架到临时目录**

```bash
cd "E:/opencode learning/05"
pnpm dlx create-tauri-app@latest zcode-count-scaffold --template react-ts --manager pnpm --yes
```

- [ ] **Step 2: 合并进已有仓库目录（保留 docs/ 与 .git/）**

```bash
cd "E:/opencode learning/05"
cp -r zcode-count-scaffold/. zcode-count/ 2>/dev/null || rsync -a --exclude .git zcode-count-scaffold/ zcode-count/
rm -rf zcode-count-scaffold
cd zcode-count && ls
```

Expected: 根目录出现 `package.json`、`src/`、`src-tauri/`，且 `docs/`、`.git/` 仍在。

- [ ] **Step 3: 安装依赖**

```bash
cd "E:/opencode learning/05/zcode-count"
pnpm install
```

- [ ] **Step 4: 调整 `src-tauri/tauri.conf.json`**

把 `productName` 改为 `zcode-count`，`identifier` 改为 `com.fufu.zcode-count`，`bundle.targets` 改为 `["msi", "nsis"]`，`app.windows[0].title` 改为 `zcode-count`、`width` 1100、`height` 720。

- [ ] **Step 5: 加 Rust 依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 增加：

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
rust_decimal = "1"
chrono = "0.4"
thiserror = "2"
```

- [ ] **Step 6: 验证前后端各自可构建**

```bash
cd "E:/opencode learning/05/zcode-count"
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: 两条命令均成功（首次编译较慢）。

- [ ] **Step 7: 提交**

```bash
cd "E:/opencode learning/05/zcode-count"
git add -A
git commit -m "chore: Tauri + React 脚手架与依赖"
```

---

### Task 2: 自有库 schema 与 DAO

**Files:**
- Create: `src-tauri/src/db/mod.rs`、`src-tauri/src/db/schema.rs`、`src-tauri/src/db/dao.rs`、`src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs`（加 `pub mod db; pub mod error;`）

**Interfaces:**
- Consumes: 无
- Produces:
  - `error::AppError`（thiserror 枚举，含 `Database(String)`、`Io(String)`、`Config(String)`）
  - `db::schema::migrate(conn: &Connection) -> Result<(), AppError>`
  - `db::dao` 自由函数（均接收 `&Connection`）：
    - `insert_record(conn, rec: &UsageRecord) -> Result<bool, AppError>`
    - `get_cursor(conn, source: &str) -> Result<Option<Cursor>, AppError>`
    - `set_cursor(conn, source: &str, last_started_at: i64, last_mtime: i64, synced_at: i64) -> Result<(), AppError>`
    - `query_summary(conn, since: i64, until: i64) -> Result<Summary, AppError>`
    - `query_logs(conn, since, until, provider: Option<&str>, limit: i64) -> Result<Vec<RequestLogRow>, AppError>`
    - `query_provider_stats(conn, since, until) -> Result<Vec<ProviderStat>, AppError>`
    - `query_model_stats(conn, since, until) -> Result<Vec<ModelStat>, AppError>`
    - `get_overrides(conn) -> Result<HashMap<String, ModelPricing>, AppError>`
    - `set_override(conn, model_id: &str, p: &ModelPricing) -> Result<(), AppError>`
  - 结构体：`UsageRecord`、`Cursor`、`Summary`、`RequestLogRow`、`ProviderStat`、`ModelStat`（`serde::Serialize`）
  - `db::OwnDb { conn: Mutex<Connection> }`：`open(path) -> Result<Self, AppError>`（建目录、`migrate`），方法逐一委托 dao。
  - 定价结构体 `pricing::ModelPricing { input, output, cache_read, cache_creation: Decimal }` 在 Task 3 定义；**本任务先在 `db/dao.rs` 中引用 `crate::pricing::ModelPricing`，Task 3 实现**。为解耦，本任务先在 `db/mod.rs` 里定义 `pub use crate::pricing::ModelPricing;` 并在 Task 3 落地前用一个占位 re-export。**执行顺序保证：先做 Task 3 的 `pricing::ModelPricing` 定义再跑本任务测试。**（见 Task 3 说明）

> 说明：为避免循环依赖，`ModelPricing` 定义放在 `pricing` 模块（Task 3）。本任务测试前需先完成 Task 3 的 Step 1。

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/db/dao.rs` 底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::migrate;
    use crate::pricing::ModelPricing;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    fn rec(id: &str, model: &str, started: i64) -> UsageRecord {
        UsageRecord {
            request_id: id.into(),
            app_type: "zcode".into(),
            provider_id: "p1".into(),
            model_id: model.into(),
            input_tokens: 1000,
            output_tokens: 500,
            reasoning_tokens: 100,
            cache_read_tokens: 800,
            cache_creation_tokens: 0,
            input_cost_usd: "0.00006".into(),
            output_cost_usd: "0.0006".into(),
            cache_read_cost_usd: "0.0000048".into(),
            cache_creation_cost_usd: "0".into(),
            total_cost_usd: "0.0006648".into(),
            priced: true,
            started_at: started,
            duration_ms: Some(1000),
            first_token_ms: Some(200),
            status: "completed".into(),
            session_id: Some("s1".into()),
            created_at: started,
        }
    }

    #[test]
    fn insert_is_idempotent() {
        let c = conn();
        assert!(insert_record(&c, &rec("a", "m1", 10)).unwrap());
        assert!(!insert_record(&c, &rec("a", "m1", 10)).unwrap());
    }

    #[test]
    fn summary_aggregates_tokens_and_cost() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        insert_record(&c, &rec("b", "m1", 20)).unwrap();
        let s = query_summary(&c, 0, 100).unwrap();
        assert_eq!(s.request_count, 2);
        assert_eq!(s.input_tokens, 2000);
        assert_eq!(s.output_tokens, 1000);
        assert_eq!(s.cache_read_tokens, 1600);
        assert_eq!(Decimal::from_str(&s.total_cost_usd).unwrap(),
                   Decimal::from_str("0.0013296").unwrap());
    }

    #[test]
    fn model_stats_groups_by_model() {
        let c = conn();
        insert_record(&c, &rec("a", "m1", 10)).unwrap();
        insert_record(&c, &rec("b", "m2", 20)).unwrap();
        let rows = query_model_stats(&c, 0, 100).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn cursor_roundtrip() {
        let c = conn();
        assert!(get_cursor(&c, "src").unwrap().is_none());
        set_cursor(&c, "src", 123, 456, 789).unwrap();
        let cur = get_cursor(&c, "src").unwrap().unwrap();
        assert_eq!((cur.last_started_at, cur.last_mtime), (123, 456));
    }

    #[test]
    fn override_roundtrip() {
        let c = conn();
        let p = ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        };
        set_override(&c, "m1", &p).unwrap();
        let map = get_overrides(&c).unwrap();
        assert_eq!(map.get("m1").unwrap().input, p.input);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml db::dao
```

Expected: 编译失败（`migrate`/`insert_record` 等未定义）。

- [ ] **Step 3: 实现 `error.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("config error: {0}")]
    Config(String),
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self { AppError::Database(e.to_string()) }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e.to_string()) }
}
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
```

- [ ] **Step 4: 实现 `db/schema.rs`**

```rust
use crate::error::AppError;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS usage_records (
          request_id TEXT PRIMARY KEY,
          app_type TEXT NOT NULL DEFAULT 'zcode',
          provider_id TEXT NOT NULL,
          model_id TEXT NOT NULL,
          input_tokens INTEGER NOT NULL DEFAULT 0,
          output_tokens INTEGER NOT NULL DEFAULT 0,
          reasoning_tokens INTEGER NOT NULL DEFAULT 0,
          cache_read_tokens INTEGER NOT NULL DEFAULT 0,
          cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
          input_cost_usd TEXT NOT NULL DEFAULT '0',
          output_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_read_cost_usd TEXT NOT NULL DEFAULT '0',
          cache_creation_cost_usd TEXT NOT NULL DEFAULT '0',
          total_cost_usd TEXT NOT NULL DEFAULT '0',
          priced INTEGER NOT NULL DEFAULT 0,
          started_at INTEGER NOT NULL,
          duration_ms INTEGER,
          first_token_ms INTEGER,
          status TEXT NOT NULL,
          session_id TEXT,
          created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_started ON usage_records(started_at);
        CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model_id);
        CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_records(provider_id);

        CREATE TABLE IF NOT EXISTS sync_cursors (
          source TEXT PRIMARY KEY,
          last_started_at INTEGER NOT NULL DEFAULT 0,
          last_mtime INTEGER NOT NULL DEFAULT 0,
          last_synced_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS pricing_overrides (
          model_id TEXT PRIMARY KEY,
          input_cost_per_million TEXT NOT NULL,
          output_cost_per_million TEXT NOT NULL,
          cache_read_cost_per_million TEXT NOT NULL,
          cache_creation_cost_per_million TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}
```

- [ ] **Step 5: 实现 `db/dao.rs`**

```rust
use crate::error::AppError;
use crate::pricing::ModelPricing;
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    pub request_id: String,
    pub app_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub input_cost_usd: String,
    pub output_cost_usd: String,
    pub cache_read_cost_usd: String,
    pub cache_creation_cost_usd: String,
    pub total_cost_usd: String,
    pub priced: bool,
    pub started_at: i64,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub status: String,
    pub session_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cursor {
    pub last_started_at: i64,
    pub last_mtime: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Summary {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub request_count: i64,
    pub session_count: i64,
    pub total_cost_usd: String,
    pub unpriced_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestLogRow {
    pub request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_cost_usd: String,
    pub priced: bool,
    pub duration_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub status: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub provider_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStat {
    pub model_id: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_cost_usd: String,
}

pub fn insert_record(conn: &Connection, r: &UsageRecord) -> Result<bool, AppError> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO usage_records (
            request_id, app_type, provider_id, model_id,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd,
            total_cost_usd, priced, started_at, duration_ms, first_token_ms,
            status, session_id, created_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
        params![
            r.request_id, r.app_type, r.provider_id, r.model_id,
            r.input_tokens, r.output_tokens, r.reasoning_tokens,
            r.cache_read_tokens, r.cache_creation_tokens,
            r.input_cost_usd, r.output_cost_usd, r.cache_read_cost_usd, r.cache_creation_cost_usd,
            r.total_cost_usd, r.priced as i64, r.started_at, r.duration_ms, r.first_token_ms,
            r.status, r.session_id, r.created_at,
        ],
    )?;
    Ok(n > 0)
}

pub fn get_cursor(conn: &Connection, source: &str) -> Result<Option<Cursor>, AppError> {
    Ok(conn
        .query_row(
            "SELECT last_started_at, last_mtime FROM sync_cursors WHERE source = ?1",
            [source],
            |row| Ok(Cursor { last_started_at: row.get(0)?, last_mtime: row.get(1)? }),
        )
        .optional()?)
}

pub fn set_cursor(conn: &Connection, source: &str, last_started_at: i64, last_mtime: i64, synced_at: i64) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO sync_cursors (source, last_started_at, last_mtime, last_synced_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source) DO UPDATE SET
           last_started_at = excluded.last_started_at,
           last_mtime = excluded.last_mtime,
           last_synced_at = excluded.last_synced_at",
        params![source, last_started_at, last_mtime, synced_at],
    )?;
    Ok(())
}

pub fn query_summary(conn: &Connection, since: i64, until: i64) -> Result<Summary, AppError> {
    let mut stmt = conn.prepare(
        "SELECT input_tokens, output_tokens, reasoning_tokens, cache_read_tokens,
                cache_creation_tokens, session_id, total_cost_usd, priced
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2",
    )?;
    let it = stmt.query_map(params![since, until], |row| {
        Ok((
            row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?, row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?, row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;

    let mut s = Summary::default();
    let mut total = Decimal::ZERO;
    let mut sessions = std::collections::HashSet::new();
    for r in it {
        let (i, o, re, cr, cc, sess, cost, priced) = r?;
        s.input_tokens += i;
        s.output_tokens += o;
        s.reasoning_tokens += re;
        s.cache_read_tokens += cr;
        s.cache_creation_tokens += cc;
        s.request_count += 1;
        if let Some(x) = sess { sessions.insert(x); }
        if priced == 0 { s.unpriced_count += 1; }
        total += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
    }
    s.session_count = sessions.len() as i64;
    s.total_cost_usd = total.normalize().to_string();
    Ok(s)
}

pub fn query_logs(conn: &Connection, since: i64, until: i64, provider: Option<&str>, limit: i64) -> Result<Vec<RequestLogRow>, AppError> {
    let mut sql = String::from(
        "SELECT request_id, provider_id, model_id, input_tokens, output_tokens,
                cache_read_tokens, total_cost_usd, priced, duration_ms, first_token_ms,
                status, started_at
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2",
    );
    if provider.is_some() {
        sql.push_str(" AND provider_id = ?3");
    }
    sql.push_str(" ORDER BY started_at DESC LIMIT ?");
    let limit_idx = if provider.is_some() { 4 } else { 3 };
    sql.push_str(&limit_idx.to_string());

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<RequestLogRow> {
        Ok(RequestLogRow {
            request_id: row.get(0)?,
            provider_id: row.get(1)?,
            model_id: row.get(2)?,
            input_tokens: row.get(3)?,
            output_tokens: row.get(4)?,
            cache_read_tokens: row.get(5)?,
            total_cost_usd: row.get(6)?,
            priced: row.get::<_, i64>(7)? != 0,
            duration_ms: row.get(8)?,
            first_token_ms: row.get(9)?,
            status: row.get(10)?,
            started_at: row.get(11)?,
        })
    };

    let mut rows = Vec::new();
    if let Some(p) = provider {
        let mut stmt = conn.prepare(&sql)?;
        let it = stmt.query_map(params![since, until, p, limit], map_row)?;
        for r in it { rows.push(r?); }
    } else {
        let mut stmt = conn.prepare(&sql)?;
        let it = stmt.query_map(params![since, until, limit], map_row)?;
        for r in it { rows.push(r?); }
    }
    Ok(rows)
}

fn group_stats(conn: &Connection, key: &str, since: i64, until: i64) -> Result<Vec<(String, i64, i64, i64, String)>, AppError> {
    let sql = format!(
        "SELECT {key}, input_tokens, output_tokens, total_cost_usd
         FROM usage_records WHERE started_at >= ?1 AND started_at <= ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let it = stmt.query_map(params![since, until], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut map: HashMap<String, (i64, i64, i64, Decimal)> = HashMap::new();
    for r in it {
        let (k, i, o, cost) = r?;
        let e = map.entry(k).or_insert((0, 0, 0, Decimal::ZERO));
        e.0 += 1;
        e.1 += i;
        e.2 += o;
        e.3 += Decimal::from_str(&cost).unwrap_or(Decimal::ZERO);
    }
    let mut out: Vec<(String, i64, i64, i64, String)> = map
        .into_iter()
        .map(|(k, (c, i, o, cost))| (k, c, i, o, cost.normalize().to_string()))
        .collect();
    // 按成本（Decimal）降序，避免字符串排序错误
    out.sort_by(|a, b| {
        let ca = Decimal::from_str(&a.4).unwrap_or(Decimal::ZERO);
        let cb = Decimal::from_str(&b.4).unwrap_or(Decimal::ZERO);
        cb.cmp(&ca)
    });
    Ok(out)
}

pub fn query_provider_stats(conn: &Connection, since: i64, until: i64) -> Result<Vec<ProviderStat>, AppError> {
    Ok(group_stats(conn, "provider_id", since, until)?
        .into_iter()
        .map(|(id, c, i, o, cost)| ProviderStat { provider_id: id, request_count: c, input_tokens: i, output_tokens: o, total_cost_usd: cost })
        .collect())
}

pub fn query_model_stats(conn: &Connection, since: i64, until: i64) -> Result<Vec<ModelStat>, AppError> {
    Ok(group_stats(conn, "model_id", since, until)?
        .into_iter()
        .map(|(id, c, i, o, cost)| ModelStat { model_id: id, request_count: c, input_tokens: i, output_tokens: o, total_cost_usd: cost })
        .collect())
}

pub fn get_overrides(conn: &Connection) -> Result<HashMap<String, ModelPricing>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT model_id, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM pricing_overrides",
    )?;
    let it = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut map = HashMap::new();
    for r in it {
        let (id, i, o, cr, cc) = r?;
        if let Ok(p) = ModelPricing::from_strings(&i, &o, &cr, &cc) {
            map.insert(id, p);
        }
    }
    Ok(map)
}

pub fn set_override(conn: &Connection, model_id: &str, p: &ModelPricing) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO pricing_overrides (model_id, input_cost_per_million, output_cost_per_million,
            cache_read_cost_per_million, cache_creation_cost_per_million)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(model_id) DO UPDATE SET
           input_cost_per_million = excluded.input_cost_per_million,
           output_cost_per_million = excluded.output_cost_per_million,
           cache_read_cost_per_million = excluded.cache_read_cost_per_million,
           cache_creation_cost_per_million = excluded.cache_creation_cost_per_million",
        params![
            model_id,
            p.input.to_string(), p.output.to_string(),
            p.cache_read.to_string(), p.cache_creation.to_string()
        ],
    )?;
    Ok(())
}
```

- [ ] **Step 6: 实现 `db/mod.rs` 与 `lib.rs` 注册**

`db/mod.rs`：

```rust
pub mod dao;
pub mod schema;

use crate::error::AppError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct OwnDb {
    pub conn: Mutex<Connection>,
}

impl OwnDb {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}
```

`lib.rs` 顶部加：

```rust
pub mod db;
pub mod error;
pub mod pricing;
```

并在 `src-tauri/src/pricing/mod.rs` 暂放 `pub struct ModelPricing`（**本任务只定义结构体，不要声明子模块**；`candidates/cost/table` 由 Task 3 创建）：

```rust
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation: Decimal,
}

impl ModelPricing {
    pub fn from_strings(i: &str, o: &str, cr: &str, cc: &str) -> Result<Self, rust_decimal::Error> {
        Ok(Self {
            input: i.parse()?,
            output: o.parse()?,
            cache_read: cr.parse()?,
            cache_creation: cc.parse()?,
        })
    }
}
```

- [ ] **Step 7: 运行测试确认通过**

```bash
cargo test --manifest-path src-tauri/Cargo.toml db::dao
```

Expected: 5 个测试 PASS。

- [ ] **Step 8: 提交**

```bash
git add -A
git commit -m "feat(db): 自有库 schema 与 DAO"
```

---

### Task 3: 定价解析与成本计算

**Files:**
- Create: `src-tauri/src/pricing/candidates.rs`、`src-tauri/src/pricing/table.rs`、`src-tauri/src/pricing/cost.rs`
- Modify: `src-tauri/src/pricing/mod.rs`

**Interfaces:**
- Consumes: `ModelPricing`（Task 2 Step 6 已定义）
- Produces:
  - `pricing::candidates::model_candidates(model_id: &str) -> Vec<String>`
  - `pricing::table::PricingTable::load(path: &Path) -> Result<Self, AppError>`；`lookup(&self, model_id: &str) -> Option<ModelPricing>`
  - `pricing::cost::calculate_cache_inclusive(input: i64, output: i64, cache_read: i64, cache_creation: i64, p: &ModelPricing) -> CostBreakdown`，`CostBreakdown { input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost: Decimal }`
  - `pricing::cc_switch_db_path() -> PathBuf`
  - `pricing::resolve(model_id: &str, table: &PricingTable, overrides: &HashMap<String, ModelPricing>) -> Option<ModelPricing>`

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/pricing/candidates.rs` 底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_provider_namespace_and_tag() {
        let c = model_candidates("openai/deepseek-v4.1-flash:high");
        assert!(c.contains(&"deepseek-v4.1-flash".to_string()));
    }

    #[test]
    fn keeps_plain_id() {
        let c = model_candidates("deepseek-v4.1-flash");
        assert_eq!(c[0], "deepseek-v4.1-flash");
    }

    #[test]
    fn placeholder_yields_empty() {
        assert!(model_candidates("unknown").is_empty());
        assert!(model_candidates("").is_empty());
    }
}
```

在 `src-tauri/src/pricing/cost.rs` 底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn p() -> ModelPricing {
        ModelPricing {
            input: Decimal::from_str("0.3").unwrap(),
            output: Decimal::from_str("1.2").unwrap(),
            cache_read: Decimal::from_str("0.006").unwrap(),
            cache_creation: Decimal::ZERO,
        }
    }

    #[test]
    fn cache_inclusive_subtracts_cache_from_input() {
        // input 含 cache：2036 里含 114944? 用合理样例
        let b = calculate_cache_inclusive(53292, 2805, 46464, 0, &p());
        // billable input = 53292 - 46464 = 6828
        assert_eq!(b.input_cost, Decimal::from_str("0.0020484").unwrap());
        assert_eq!(b.output_cost, Decimal::from_str("0.003366").unwrap());
        assert_eq!(b.cache_read_cost, Decimal::from_str("0.000278784").unwrap());
        assert_eq!(b.total_cost, Decimal::from_str("0.005693184").unwrap());
    }

    #[test]
    fn saturating_when_cache_exceeds_input() {
        let b = calculate_cache_inclusive(100, 10, 500, 0, &p());
        assert_eq!(b.input_cost, Decimal::ZERO);
    }
}
```

在 `src-tauri/src/pricing/table.rs` 底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn exact_then_prefix_match() {
        let mut t = PricingTable::from_rows(vec![
            ("deepseek-v4-flash".into(), ModelPricing {
                input: Decimal::from_str("0.3").unwrap(),
                output: Decimal::from_str("1.2").unwrap(),
                cache_read: Decimal::from_str("0.006").unwrap(),
                cache_creation: Decimal::ZERO,
            }),
        ]);
        assert!(t.lookup("deepseek-v4-flash").is_some());
        // 前缀：deepseek-v4-flash-0731 -> deepseek-v4-flash
        assert!(t.lookup("deepseek-v4-flash-0731").is_some());
        assert!(t.lookup("nope").is_none());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml pricing
```

Expected: 编译失败（函数未定义）。

- [ ] **Step 3: 实现 `candidates.rs`**

```rust
/// 生成模型名候选，供定价精确/前缀匹配。参照 cc-switch 的归一化语义。
pub fn model_candidates(model_id: &str) -> Vec<String> {
    let cleaned = clean(model_id);
    if is_placeholder(&cleaned) {
        return Vec::new();
    }
    let mut out = Vec::new();
    push_unique(&mut out, cleaned.clone());
    if let Some(s) = strip_date_suffix(&cleaned) {
        push_unique(&mut out, s);
    }
    out
}

fn clean(model_id: &str) -> String {
    let s = model_id.rsplit_once('/').map_or(model_id, |(_, r)| r);
    s.split(':').next().unwrap_or(s).trim().to_ascii_lowercase()
}

fn is_placeholder(s: &str) -> bool {
    s.is_empty() || matches!(s, "unknown" | "null" | "none")
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !s.is_empty() && !v.contains(&s) {
        v.push(s);
    }
}

/// 去掉尾部纯数字后缀，如 -0731 或 -20250101。
fn strip_date_suffix(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts[parts.len() - 1];
    if last.len() >= 4 && last.chars().all(|c| c.is_ascii_digit()) {
        return Some(parts[..parts.len() - 1].join("-"));
    }
    None
}
```

- [ ] **Step 4: 实现 `cost.rs`**

```rust
use super::ModelPricing;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct CostBreakdown {
    pub input_cost: Decimal,
    pub output_cost: Decimal,
    pub cache_read_cost: Decimal,
    pub cache_creation_cost: Decimal,
    pub total_cost: Decimal,
}

/// ZCode 的 input_tokens 为 cache-inclusive（总输入），需先扣 cache 再按输入价计费。
pub fn calculate_cache_inclusive(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
    p: &ModelPricing,
) -> CostBreakdown {
    let million = Decimal::from(1_000_000);
    let billable_input = input.saturating_sub(cache_read).saturating_sub(cache_creation);
    let input_cost = Decimal::from(billable_input) * p.input / million;
    let output_cost = Decimal::from(output) * p.output / million;
    let cache_read_cost = Decimal::from(cache_read) * p.cache_read / million;
    let cache_creation_cost = Decimal::from(cache_creation) * p.cache_creation / million;
    let total_cost = input_cost + output_cost + cache_read_cost + cache_creation_cost;
    CostBreakdown { input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost }
}
```

- [ ] **Step 5: 实现 `table.rs`**

```rust
use super::candidates::model_candidates;
use super::ModelPricing;
use crate::error::AppError;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub struct PricingTable {
    rows: Vec<(String, ModelPricing)>,
}

impl PricingTable {
    pub fn from_rows(mut rows: Vec<(String, ModelPricing)>) -> Self {
        rows.sort_by(|a, b| a.0.len().cmp(&b.0.len()));
        Self { rows }
    }

    pub fn load(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| AppError::Database(format!("打开 cc-switch 库失败: {e}")))?;
        let mut stmt = conn.prepare(
            "SELECT model_id, input_cost_per_million, output_cost_per_million,
                    cache_read_cost_per_million, cache_creation_cost_per_million
             FROM model_pricing",
        )?;
        let it = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut rows = Vec::new();
        for r in it {
            let (id, i, o, cr, cc) = r?;
            if let Ok(p) = ModelPricing::from_strings(&i, &o, &cr, &cc) {
                rows.push((id.to_ascii_lowercase(), p));
            }
        }
        Ok(Self::from_rows(rows))
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn lookup(&self, model_id: &str) -> Option<ModelPricing> {
        let candidates = model_candidates(model_id);
        for c in &candidates {
            if let Some((_, p)) = self.rows.iter().find(|(k, _)| k == c) {
                return Some(p.clone());
            }
        }
        for c in &candidates {
            let prefix = format!("{c}-");
            if let Some((_, p)) = self.rows.iter().find(|(k, _)| k.starts_with(&prefix)) {
                return Some(p.clone());
            }
        }
        None
    }
}
```

- [ ] **Step 6: 实现 `pricing/mod.rs`（补 `resolve` 与路径）**

```rust
pub mod candidates;
pub mod cost;
pub mod table;

use crate::error::AppError;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub input: Decimal,
    pub output: Decimal,
    pub cache_read: Decimal,
    pub cache_creation: Decimal,
}

impl ModelPricing {
    pub fn from_strings(i: &str, o: &str, cr: &str, cc: &str) -> Result<Self, rust_decimal::Error> {
        Ok(Self {
            input: i.parse()?,
            output: o.parse()?,
            cache_read: cr.parse()?,
            cache_creation: cc.parse()?,
        })
    }
}

/// cc-switch 定价库路径：%USERPROFILE%\.cc-switch\cc-switch.db
pub fn cc_switch_db_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".cc-switch").join("cc-switch.db")
}

pub fn resolve(
    model_id: &str,
    table: &table::PricingTable,
    overrides: &HashMap<String, ModelPricing>,
) -> Option<ModelPricing> {
    if let Some(p) = overrides.get(model_id) {
        return Some(p.clone());
    }
    table.lookup(model_id)
}
```

- [ ] **Step 7: 运行测试确认通过**

```bash
cargo test --manifest-path src-tauri/Cargo.toml pricing
```

Expected: 全部 PASS。

- [ ] **Step 8: 提交**

```bash
git add -A
git commit -m "feat(pricing): 定价解析与 cache-inclusive 成本计算"
```

---

### Task 4: ZCode 读取与增量同步

**Files:**
- Create: `src-tauri/src/zcode/mod.rs`、`src-tauri/src/zcode/sync.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `db::dao::*`、`db::schema::migrate`、`pricing::{resolve, table::PricingTable, cost::calculate_cache_inclusive, ModelPricing}`、`error::AppError`
- Produces:
  - `zcode::zcode_db_path() -> PathBuf`
  - `zcode::sync::sync(conn: &Connection, zcode_path: &Path, pricing: &PricingTable, overrides: &HashMap<String, ModelPricing>) -> Result<SyncReport, AppError>`
  - `SyncReport { scanned: i64, imported: i64, skipped: i64, unpriced: i64, last_started_at: i64, zcode_found: bool }`（`Serialize`）

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/zcode/sync.rs` 底部：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::migrate;
    use crate::pricing::{table::PricingTable, ModelPricing};
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn setup_zcode(path: &std::path::Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE model_usage (
                id TEXT PRIMARY KEY, provider_id TEXT, model_id TEXT,
                status TEXT, started_at INTEGER, duration_ms INTEGER,
                time_to_first_token_ms INTEGER, input_tokens INTEGER,
                output_tokens INTEGER, reasoning_tokens INTEGER,
                cache_read_input_tokens INTEGER, cache_creation_input_tokens INTEGER,
                session_id TEXT);",
        ).unwrap();
        c.execute(
            "INSERT INTO model_usage VALUES
             ('r1','p1','deepseek-v4-flash','completed',100,1000,200,1000,500,100,800,0,'s1'),
             ('r2','p1','deepseek-v4-flash-0731','error',200,900,150,2000,0,0,0,0,'s1')",
            [],
        ).unwrap();
    }

    fn table() -> PricingTable {
        PricingTable::from_rows(vec![(
            "deepseek-v4-flash".into(),
            ModelPricing {
                input: Decimal::from_str("0.3").unwrap(),
                output: Decimal::from_str("1.2").unwrap(),
                cache_read: Decimal::from_str("0.006").unwrap(),
                cache_creation: Decimal::ZERO,
            },
        )])
    }

    #[test]
    fn sync_imports_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("zc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("db.sqlite");
        let _ = std::fs::remove_file(&zpath);
        setup_zcode(&zpath);

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let overrides = HashMap::new();

        let r1 = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(r1.imported, 2);
        assert_eq!(r1.unpriced, 0);

        let r2 = sync(&conn, &zpath, &table(), &overrides).unwrap();
        assert_eq!(r2.imported, 0); // 幂等

        let logs = crate::db::dao::query_logs(&conn, 0, 1000, None, 100).unwrap();
        assert_eq!(logs.len(), 2);
        // r1: billable input = 1000-800 = 200 -> 200*0.3/1e6 + 500*1.2/1e6 + 800*0.006/1e6
        let r1row = logs.iter().find(|l| l.request_id == "r1").unwrap();
        assert_eq!(r1row.total_cost_usd, "0.0006648");

        let _ = std::fs::remove_file(&zpath);
    }

    #[test]
    fn missing_zcode_db_is_reported() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let r = sync(&conn, std::path::Path::new("C:/definitely/not/here.sqlite"),
                     &PricingTable::from_rows(vec![]), &HashMap::new()).unwrap();
        assert!(!r.zcode_found);
        assert_eq!(r.imported, 0);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml zcode
```

Expected: 编译失败。

- [ ] **Step 3: 实现 `zcode/mod.rs`**

```rust
pub mod sync;

use std::path::PathBuf;

/// ZCode 用量库路径：%USERPROFILE%\.zcode\cli\db\db.sqlite
pub fn zcode_db_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".zcode").join("cli").join("db").join("db.sqlite")
}
```

- [ ] **Step 4: 实现 `zcode/sync.rs`**

```rust
use crate::db::dao::{self, UsageRecord};
use crate::error::AppError;
use crate::pricing::{cost::calculate_cache_inclusive, resolve, table::PricingTable, ModelPricing};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncReport {
    pub scanned: i64,
    pub imported: i64,
    pub skipped: i64,
    pub unpriced: i64,
    pub last_started_at: i64,
    pub zcode_found: bool,
}

struct ZRow {
    id: String,
    provider_id: String,
    model_id: String,
    status: String,
    started_at: i64,
    duration_ms: Option<i64>,
    first_token_ms: Option<i64>,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_read: i64,
    cache_creation: i64,
    session_id: Option<String>,
}

pub fn sync(
    conn: &Connection,
    zcode_path: &Path,
    pricing: &PricingTable,
    overrides: &HashMap<String, ModelPricing>,
) -> Result<SyncReport, AppError> {
    if !zcode_path.exists() {
        return Ok(SyncReport { zcode_found: false, ..Default::default() });
    }
    let source = zcode_path.to_string_lossy().to_string();
    let mtime = max_mtime(zcode_path);
    let cursor = dao::get_cursor(conn, &source)?;
    let since = cursor.as_ref().map_or(0, |c| c.last_started_at);

    let zconn = Connection::open_with_flags(zcode_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| AppError::Database(format!("打开 ZCode 库失败: {e}")))?;

    let rows = query_rows(&zconn, since)?;
    let mut report = SyncReport { zcode_found: true, scanned: rows.len() as i64, ..Default::default() };
    let mut max_started = since;

    for r in &rows {
        if r.started_at > max_started {
            max_started = r.started_at;
        }
        let cost = resolve(&r.model_id, pricing, overrides)
            .map(|p| calculate_cache_inclusive(r.input_tokens, r.output_tokens, r.cache_read, r.cache_creation, &p));
        let priced = cost.is_some();
        if !priced {
            report.unpriced += 1;
        }
        let (ic, oc, crc, ccc, tc) = match &cost {
            Some(c) => (c.input_cost.to_string(), c.output_cost.to_string(),
                        c.cache_read_cost.to_string(), c.cache_creation_cost.to_string(),
                        c.total_cost.to_string()),
            None => ("0".into(), "0".into(), "0".into(), "0".into(), "0".into()),
        };
        let rec = UsageRecord {
            request_id: r.id.clone(),
            app_type: "zcode".into(),
            provider_id: r.provider_id.clone(),
            model_id: r.model_id.clone(),
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            reasoning_tokens: r.reasoning_tokens,
            cache_read_tokens: r.cache_read,
            cache_creation_tokens: r.cache_creation,
            input_cost_usd: ic,
            output_cost_usd: oc,
            cache_read_cost_usd: crc,
            cache_creation_cost_usd: ccc,
            total_cost_usd: tc,
            priced,
            started_at: r.started_at,
            duration_ms: r.duration_ms,
            first_token_ms: r.first_token_ms,
            status: r.status.clone(),
            session_id: r.session_id.clone(),
            created_at: r.started_at,
        };
        if dao::insert_record(conn, &rec)? {
            report.imported += 1;
        } else {
            report.skipped += 1;
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    dao::set_cursor(conn, &source, max_started, mtime, now)?;
    report.last_started_at = max_started;
    Ok(report)
}

fn max_mtime(path: &Path) -> i64 {
    let mut m = file_mtime(path);
    let wal = path.with_extension("sqlite-wal");
    m = m.max(file_mtime(&wal));
    m
}

fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn query_rows(conn: &Connection, since: i64) -> Result<Vec<ZRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider_id, model_id, status, started_at, duration_ms,
                time_to_first_token_ms, input_tokens, output_tokens, reasoning_tokens,
                cache_read_input_tokens, cache_creation_input_tokens, session_id
         FROM model_usage WHERE started_at >= ?1 ORDER BY started_at ASC",
    )?;
    let it = stmt.query_map([since], |row| {
        Ok(ZRow {
            id: row.get(0)?,
            provider_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            model_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            status: row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "unknown".into()),
            started_at: row.get(4)?,
            duration_ms: row.get(5)?,
            first_token_ms: row.get(6)?,
            input_tokens: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            output_tokens: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            reasoning_tokens: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
            cache_read: row.get::<_, Option<i64>>(10)?.unwrap_or(0),
            cache_creation: row.get::<_, Option<i64>>(11)?.unwrap_or(0),
            session_id: row.get(12)?,
        })
    })?;
    let mut out = Vec::new();
    for r in it { out.push(r?); }
    Ok(out)
}
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --manifest-path src-tauri/Cargo.toml zcode
```

Expected: 2 个测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add -A
git commit -m "feat(zcode): 只读读取 model_usage 并增量同步"
```

---

### Task 5: Tauri commands 与后端装配

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`（注册 state、invoke_handler）

**Interfaces:**
- Consumes: `db::{OwnDb, dao}`、`pricing::{cc_switch_db_path, table::PricingTable}`、`zcode::{zcode_db_path, sync::sync}`
- Produces（前端调用的 command 名与签名）：
  - `sync_usage() -> SyncStatus`
  - `get_summary(since: i64, until: i64) -> Summary`
  - `list_logs(since: i64, until: i64, provider: Option<String>, limit: i64) -> Vec<RequestLogRow>`
  - `get_provider_stats(since, until) -> Vec<ProviderStat>`
  - `get_model_stats(since, until) -> Vec<ModelStat>`
  - `set_price_override(model_id: String, input: String, output: String, cache_read: String, cache_creation: String) -> ()`
  - `SyncStatus { zcode_found, pricing_found, imported, skipped, unpriced, last_synced_at, last_error: Option<String> }`

- [ ] **Step 1: 实现 `commands.rs`**

```rust
use crate::db::{dao, OwnDb};
use crate::pricing::{cc_switch_db_path, table::PricingTable};
use crate::zcode::{sync::sync, zcode_db_path};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub db: OwnDb,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncStatus {
    pub zcode_found: bool,
    pub pricing_found: bool,
    pub imported: i64,
    pub skipped: i64,
    pub unpriced: i64,
    pub last_synced_at: i64,
    pub last_error: Option<String>,
}

#[tauri::command]
pub fn sync_usage(state: State<'_, Mutex<AppState>>) -> Result<SyncStatus, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;

    let overrides = dao::get_overrides(&conn).map_err(|e| e.to_string())?;

    let ccp = cc_switch_db_path();
    let (pricing, pricing_found) = if ccp.exists() {
        match PricingTable::load(&ccp) {
            Ok(t) if !t.is_empty() => (t, true),
            _ => (PricingTable::from_rows(vec![]), false),
        }
    } else {
        (PricingTable::from_rows(vec![]), false)
    };

    let report = sync(&conn, &zcode_db_path(), &pricing, &overrides).map_err(|e| e.to_string())?;
    Ok(SyncStatus {
        zcode_found: report.zcode_found,
        pricing_found,
        imported: report.imported,
        skipped: report.skipped,
        unpriced: report.unpriced,
        last_synced_at: chrono::Utc::now().timestamp_millis(),
        last_error: None,
    })
}

#[tauri::command]
pub fn get_summary(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<dao::Summary, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_summary(&conn, since, until).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_logs(state: State<'_, Mutex<AppState>>, since: i64, until: i64, provider: Option<String>, limit: i64) -> Result<Vec<dao::RequestLogRow>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_logs(&conn, since, until, provider.as_deref(), limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_provider_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<Vec<dao::ProviderStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_provider_stats(&conn, since, until).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_model_stats(state: State<'_, Mutex<AppState>>, since: i64, until: i64) -> Result<Vec<dao::ModelStat>, String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    dao::query_model_stats(&conn, since, until).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_price_override(state: State<'_, Mutex<AppState>>, model_id: String, input: String, output: String, cache_read: String, cache_creation: String) -> Result<(), String> {
    let app = state.lock().map_err(|e| e.to_string())?;
    let conn = app.db.conn.lock().map_err(|e| e.to_string())?;
    let p = crate::pricing::ModelPricing::from_strings(&input, &output, &cache_read, &cache_creation)
        .map_err(|e| e.to_string())?;
    dao::set_override(&conn, &model_id, &p).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 在 `lib.rs` 的 `run()` 里装配 state 与 handler**

在 `tauri::Builder::default()` 链上加入：

```rust
.setup(|app| {
    let db_path = crate::db::default_db_path();
    let own = crate::db::OwnDb::open(&db_path).map_err(|e| e.to_string())?;
    app.manage(std::sync::Mutex::new(crate::commands::AppState { db: own }));
    Ok(())
})
.invoke_handler(tauri::generate_handler![
    crate::commands::sync_usage,
    crate::commands::get_summary,
    crate::commands::list_logs,
    crate::commands::get_provider_stats,
    crate::commands::get_model_stats,
    crate::commands::set_price_override,
])
```

并在 `db/mod.rs` 加：

```rust
pub fn default_db_path() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    std::path::PathBuf::from(home).join(".zcode-count").join("zcode-count.db")
}
```

同时确保 `lib.rs` 声明 `pub mod commands;`。

- [ ] **Step 3: 编译检查**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: 通过。

- [ ] **Step 4: 全量测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: 全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "feat(commands): Tauri commands 与后端装配"
```

---

### Task 6: 前端 API 层、汇总卡与工具栏

**Files:**
- Create: `src/lib/api.ts`、`src/lib/format.ts`、`src/lib/format.test.ts`、`src/components/Toolbar.tsx`、`src/components/SummaryCards.tsx`
- Modify: `src/App.tsx`、`src/index.css`、`package.json`（加 vitest）

**Interfaces:**
- Consumes: Task 5 的 command 名
- Produces: `api.syncUsage() / getSummary() / listLogs() / getProviderStats() / getModelStats()`、`Range` 类型、`rangeToWindow(range) -> {since, until}`、格式化函数

- [ ] **Step 1: 安装 vitest 并加测试脚本**

```bash
cd "E:/opencode learning/05/zcode-count"
pnpm add -D vitest
```

`package.json` 的 `scripts` 加 `"test": "vitest run"`。

- [ ] **Step 2: 写失败测试 `src/lib/format.test.ts`**

```ts
import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, rangeToWindow } from "./format";

describe("formatTokens", () => {
  it("formats thousands and millions", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1500)).toBe("1.5K");
    expect(formatTokens(2_300_000)).toBe("2.3M");
  });
});

describe("formatCost", () => {
  it("shows 4 decimals and dash when unpriced", () => {
    expect(formatCost("0.0006648", true)).toBe("$0.0007");
    expect(formatCost("0", false)).toBe("—");
  });
});

describe("rangeToWindow", () => {
  it("today covers from local midnight", () => {
    const { since, until } = rangeToWindow("today");
    expect(until).toBeGreaterThan(since);
  });
});
```

- [ ] **Step 3: 运行测试确认失败**

```bash
pnpm test
```

Expected: FAIL（模块不存在）。

- [ ] **Step 4: 实现 `src/lib/format.ts`**

```ts
export type Range = "all" | "today" | "7d" | "30d";

export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function formatCost(usd: string, priced: boolean): string {
  if (!priced) return "—";
  const v = Number(usd);
  if (!Number.isFinite(v)) return "—";
  return `$${v.toFixed(4)}`;
}

export function formatTime(ms: number): string {
  const d = new Date(ms);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${p(d.getMonth() + 1)}/${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function rangeToWindow(range: Range): { since: number; until: number } {
  const until = Date.now();
  if (range === "all") return { since: 0, until };
  const now = new Date();
  let since: number;
  if (range === "today") {
    since = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  } else {
    const days = range === "7d" ? 7 : 30;
    since = until - days * 24 * 60 * 60 * 1000;
  }
  return { since, until };
}
```

- [ ] **Step 5: 实现 `src/lib/api.ts`**

```ts
import { invoke } from "@tauri-apps/api/core";

export interface Summary {
  input_tokens: number; output_tokens: number; reasoning_tokens: number;
  cache_read_tokens: number; cache_creation_tokens: number;
  request_count: number; session_count: number;
  total_cost_usd: string; unpriced_count: number;
}
export interface RequestLogRow {
  request_id: string; provider_id: string; model_id: string;
  input_tokens: number; output_tokens: number; cache_read_tokens: number;
  total_cost_usd: string; priced: boolean;
  duration_ms: number | null; first_token_ms: number | null;
  status: string; started_at: number;
}
export interface ProviderStat {
  provider_id: string; request_count: number;
  input_tokens: number; output_tokens: number; total_cost_usd: string;
}
export interface ModelStat {
  model_id: string; request_count: number;
  input_tokens: number; output_tokens: number; total_cost_usd: string;
}
export interface SyncStatus {
  zcode_found: boolean; pricing_found: boolean;
  imported: number; skipped: number; unpriced: number;
  last_synced_at: number; last_error: string | null;
}

export const syncUsage = () => invoke<SyncStatus>("sync_usage");
export const getSummary = (since: number, until: number) => invoke<Summary>("get_summary", { since, until });
export const listLogs = (since: number, until: number, provider: string | null, limit: number) =>
  invoke<RequestLogRow[]>("list_logs", { since, until, provider, limit });
export const getProviderStats = (since: number, until: number) => invoke<ProviderStat[]>("get_provider_stats", { since, until });
export const getModelStats = (since: number, until: number) => invoke<ModelStat[]>("get_model_stats", { since, until });
```

- [ ] **Step 6: 实现 `Toolbar.tsx` 与 `SummaryCards.tsx`**

`Toolbar.tsx`：

```tsx
import { Range } from "../lib/format";
import { SyncStatus } from "../lib/api";

const RANGES: { id: Range; label: string }[] = [
  { id: "all", label: "全部" }, { id: "today", label: "当天" },
  { id: "7d", label: "7 天" }, { id: "30d", label: "30 天" },
];

export function Toolbar(props: {
  range: Range; onRange: (r: Range) => void;
  status: SyncStatus | null; onRefresh: () => void; loading: boolean;
}) {
  return (
    <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-3">
      <div className="flex gap-1">
        {RANGES.map((r) => (
          <button key={r.id}
            onClick={() => props.onRange(r.id)}
            className={`rounded px-3 py-1 text-sm ${props.range === r.id ? "bg-blue-600 text-white" : "bg-gray-100 text-gray-700"}`}>
            {r.label}
          </button>
        ))}
      </div>
      <button onClick={props.onRefresh} disabled={props.loading}
        className="rounded bg-gray-800 px-3 py-1 text-sm text-white disabled:opacity-50">
        {props.loading ? "同步中…" : "刷新"}
      </button>
      <span className="text-xs text-gray-500">
        {props.status?.last_synced_at
          ? `上次同步 ${new Date(props.status.last_synced_at).toLocaleTimeString()}`
          : "未同步"}
      </span>
      {props.status && !props.status.zcode_found && (
        <span className="text-xs text-red-600">未找到 ZCode 用量库</span>
      )}
      {props.status && props.status.zcode_found && !props.status.pricing_found && (
        <span className="text-xs text-amber-600">未找到 cc-switch 定价，成本显示 —</span>
      )}
    </div>
  );
}
```

`SummaryCards.tsx`：

```tsx
import { Summary } from "../lib/api";
import { formatTokens, formatCost } from "../lib/format";

function Card({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-gray-200 p-3">
      <div className="text-xs text-gray-500">{label}</div>
      <div className="text-lg font-semibold text-gray-900">{value}</div>
    </div>
  );
}

export function SummaryCards({ summary }: { summary: Summary | null }) {
  if (!summary) return null;
  return (
    <div className="grid grid-cols-3 gap-3 px-4 py-3 md:grid-cols-6">
      <Card label="总 token" value={formatTokens(summary.input_tokens + summary.output_tokens)} />
      <Card label="输入" value={formatTokens(summary.input_tokens)} />
      <Card label="输出" value={formatTokens(summary.output_tokens)} />
      <Card label="缓存读取" value={formatTokens(summary.cache_read_tokens)} />
      <Card label="请求数" value={String(summary.request_count)} />
      <Card label="总成本" value={formatCost(summary.total_cost_usd, summary.unpriced_count < summary.request_count)} />
    </div>
  );
}
```

- [ ] **Step 7: 改写 `App.tsx`（本任务先接汇总 + 工具栏，页签内容下个任务）**

```tsx
import { useEffect, useState } from "react";
import { syncUsage, getSummary, Summary, SyncStatus } from "./lib/api";
import { Range, rangeToWindow } from "./lib/format";
import { Toolbar } from "./components/Toolbar";
import { SummaryCards } from "./components/SummaryCards";

export default function App() {
  const [range, setRange] = useState<Range>("all");
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [loading, setLoading] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      const st = await syncUsage();
      setStatus(st);
      const { since, until } = rangeToWindow(range);
      setSummary(await getSummary(since, until));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, [range]);

  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <Toolbar range={range} onRange={setRange} status={status} onRefresh={refresh} loading={loading} />
      <SummaryCards summary={summary} />
    </div>
  );
}
```

- [ ] **Step 8: 运行前端测试与构建**

```bash
pnpm test && pnpm build
```

Expected: 测试 PASS，构建成功。

- [ ] **Step 9: 提交**

```bash
git add -A
git commit -m "feat(ui): API 层、汇总卡与工具栏"
```

---

### Task 7: 请求日志页签

**Files:**
- Create: `src/components/Tabs.tsx`、`src/components/RequestLogTable.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `api.listLogs`、`RequestLogRow`、`format*`
- Produces: 页签容器与请求日志表

- [ ] **Step 1: 实现 `Tabs.tsx`**

```tsx
export type TabId = "logs" | "providers" | "models";

const TABS: { id: TabId; label: string }[] = [
  { id: "logs", label: "请求日志" },
  { id: "providers", label: "Provider 统计" },
  { id: "models", label: "模型统计" },
];

export function Tabs({ active, onChange }: { active: TabId; onChange: (t: TabId) => void }) {
  return (
    <div className="flex gap-1 border-b border-gray-200 px-4 pt-3">
      {TABS.map((t) => (
        <button key={t.id} onClick={() => onChange(t.id)}
          className={`rounded-t px-4 py-2 text-sm ${active === t.id ? "border border-b-0 border-gray-200 bg-white font-medium text-blue-600" : "text-gray-500"}`}>
          {t.label}
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: 实现 `RequestLogTable.tsx`**

```tsx
import { RequestLogRow } from "../lib/api";
import { formatTokens, formatCost, formatTime } from "../lib/format";

const COLS = ["时间", "供应商", "计费模型", "输入", "输出", "总成本", "用时/首字", "状态"];

export function RequestLogTable({ rows }: { rows: RequestLogRow[] }) {
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            {COLS.map((c) => <th key={c} className="py-2 pr-4 font-medium">{c}</th>)}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.request_id} className="border-b border-gray-100">
              <td className="py-2 pr-4 whitespace-nowrap">{formatTime(r.started_at)}</td>
              <td className="py-2 pr-4">{r.provider_id}</td>
              <td className="py-2 pr-4">{r.model_id}</td>
              <td className="py-2 pr-4">
                <div>{r.input_tokens.toLocaleString()}</div>
                <div className="text-xs text-gray-400">R{r.cache_read_tokens.toLocaleString()}</div>
              </td>
              <td className="py-2 pr-4">{r.output_tokens.toLocaleString()}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, r.priced)}</td>
              <td className="py-2 pr-4 whitespace-nowrap text-gray-500">
                {r.duration_ms != null ? `${(r.duration_ms / 1000).toFixed(1)}s` : "—"}
                {r.first_token_ms != null ? ` / ${(r.first_token_ms / 1000).toFixed(1)}s` : ""}
              </td>
              <td className="py-2 pr-4">
                <span className={r.status === "completed" ? "text-green-600" : r.status === "error" ? "text-red-600" : "text-gray-500"}>
                  {r.status}
                </span>
              </td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr><td colSpan={COLS.length} className="py-6 text-center text-gray-400">暂无数据</td></tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
```

- [ ] **Step 3: 在 `App.tsx` 接入页签与日志加载**

```tsx
// 新增 state
const [tab, setTab] = useState<TabId>("logs");
const [logs, setLogs] = useState<RequestLogRow[]>([]);

// refresh() 内，汇总之后追加：
const { since, until } = rangeToWindow(range);
setLogs(await listLogs(since, until, null, 500));

// JSX：SummaryCards 之后
<Tabs active={tab} onChange={setTab} />
{tab === "logs" && <RequestLogTable rows={logs} />}
```

（导入相应组件与类型。）

- [ ] **Step 4: 构建验证**

```bash
pnpm build
```

Expected: 成功。

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "feat(ui): 请求日志页签"
```

---

### Task 8: Provider 统计与模型统计页签

**Files:**
- Create: `src/components/ProviderStatsTable.tsx`、`src/components/ModelStatsTable.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `api.getProviderStats`、`api.getModelStats`、`format*`
- Produces: 两张统计表（模型统计含「所有模型」汇总行 + 按成本排序）

- [ ] **Step 1: 实现 `ProviderStatsTable.tsx`**

```tsx
import { ProviderStat } from "../lib/api";
import { formatTokens, formatCost } from "../lib/format";

export function ProviderStatsTable({ rows }: { rows: ProviderStat[] }) {
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            <th className="py-2 pr-4 font-medium">供应商</th>
            <th className="py-2 pr-4 font-medium">请求数</th>
            <th className="py-2 pr-4 font-medium">输入</th>
            <th className="py-2 pr-4 font-medium">输出</th>
            <th className="py-2 pr-4 font-medium">总成本</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.provider_id} className="border-b border-gray-100">
              <td className="py-2 pr-4">{r.provider_id}</td>
              <td className="py-2 pr-4">{r.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(r.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(r.output_tokens)}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, true)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

- [ ] **Step 2: 实现 `ModelStatsTable.tsx`（含所有模型汇总行）**

```tsx
import { ModelStat } from "../lib/api";
import { formatTokens, formatCost } from "../lib/format";

export function ModelStatsTable({ rows, summary }: {
  rows: ModelStat[];
  summary: { input_tokens: number; output_tokens: number; total_cost_usd: string; request_count: number } | null;
}) {
  const sorted = [...rows].sort((a, b) => Number(b.total_cost_usd) - Number(a.total_cost_usd));
  return (
    <div className="overflow-auto px-4 py-3">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 text-left text-xs text-gray-500">
            <th className="py-2 pr-4 font-medium">模型</th>
            <th className="py-2 pr-4 font-medium">请求数</th>
            <th className="py-2 pr-4 font-medium">输入</th>
            <th className="py-2 pr-4 font-medium">输出</th>
            <th className="py-2 pr-4 font-medium">总成本</th>
          </tr>
        </thead>
        <tbody>
          {summary && (
            <tr className="border-b border-gray-200 bg-gray-50 font-medium">
              <td className="py-2 pr-4">所有模型</td>
              <td className="py-2 pr-4">{summary.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(summary.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(summary.output_tokens)}</td>
              <td className="py-2 pr-4">{formatCost(summary.total_cost_usd, true)}</td>
            </tr>
          )}
          {sorted.map((r) => (
            <tr key={r.model_id} className="border-b border-gray-100">
              <td className="py-2 pr-4">{r.model_id}</td>
              <td className="py-2 pr-4">{r.request_count}</td>
              <td className="py-2 pr-4">{formatTokens(r.input_tokens)}</td>
              <td className="py-2 pr-4">{formatTokens(r.output_tokens)}</td>
              <td className="py-2 pr-4 font-medium">{formatCost(r.total_cost_usd, true)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

- [ ] **Step 3: 在 `App.tsx` 接入两个页签**

```tsx
// 新增 state
const [providerStats, setProviderStats] = useState<ProviderStat[]>([]);
const [modelStats, setModelStats] = useState<ModelStat[]>([]);

// refresh() 内追加：
setProviderStats(await getProviderStats(since, until));
setModelStats(await getModelStats(since, until));

// JSX：
{tab === "providers" && <ProviderStatsTable rows={providerStats} />}
{tab === "models" && <ModelStatsTable rows={modelStats} summary={summary} />}
```

- [ ] **Step 4: 构建与测试**

```bash
pnpm test && pnpm build
```

Expected: PASS + 构建成功。

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "feat(ui): Provider 统计与模型统计页签"
```

---

### Task 9: CI、README 与首次发布

**Files:**
- Create: `.github/workflows/build.yml`、`README.md`

**Interfaces:**
- Consumes: 完整项目
- Produces: Windows 安装包 artifact；公开仓库

- [ ] **Step 1: 写 `.github/workflows/build.yml`**

```yaml
name: build

on:
  workflow_dispatch:
  push:
    tags:
      - "v*"

jobs:
  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
      - uses: swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - run: pnpm install --frozen-lockfile
      - run: pnpm test
      - run: pnpm tauri build

      - uses: actions/upload-artifact@v4
        with:
          name: zcode-count-windows
          path: |
            src-tauri/target/release/bundle/msi/*.msi
            src-tauri/target/release/bundle/nsis/*.exe
```

- [ ] **Step 2: 写 `README.md`**

内容需包含：项目用途（监控本机 ZCode 用量）、前置条件（**必须安装 cc-switch 以获取定价**；ZCode 需有本地用量库）、数据源路径、开发命令（`pnpm install` / `pnpm tauri dev` / `pnpm test`）、构建与下载说明。

- [ ] **Step 3: 本地完整构建验证**

```bash
cd "E:/opencode learning/05/zcode-count"
pnpm tauri build
```

Expected: `src-tauri/target/release/bundle/` 下生成 `.msi` 与 `setup.exe`。

- [ ] **Step 4: 创建公开仓库并推送**

```bash
cd "E:/opencode learning/05/zcode-count"
git add -A
git commit -m "ci: Windows 构建工作流与 README"
gh repo create zcode-count --public --source . --remote origin --push
```

- [ ] **Step 5: 打 tag 触发 CI 并确认产物**

```bash
git tag v0.1.0
git push origin v0.1.0
gh run watch
```

Expected: `build` workflow 成功，artifact `zcode-count-windows` 含 `.msi` 与 `.exe`。

- [ ] **Step 6: 人工验收**

打开安装后的 zcode-count，确认：汇总卡 token/请求数/成本有值；三个页签可切换；与 ZCode 自带「App Usage」页的总 token 数量级一致。

---

## Self-Review

**Spec coverage：**
- 数据源与路径 → Global Constraints + Task 2/3/4 ✅
- ZCode `model_usage` 只读读取 → Task 4 ✅
- cc-switch 定价复用 + 缺失降级 → Task 3 + Task 5/6 ✅
- cache-inclusive 成本计算 → Task 3 ✅
- 自有库累积历史 + 水位线幂等 → Task 2 + Task 4 ✅
- 汇总卡 + 三页签（请求日志/Provider/模型） → Task 6/7/8 ✅
- 仅 Windows、msi/nsis → Task 1 + Task 9 ✅
- GitHub Actions 编译 → Task 9 ✅
- 错误处理（缺库/缺定价/WAL） → Task 4（WAL mtime、missing 报告）+ Task 5/6（提示） ✅
- 测试（成本/解析/sync 幂等 + 前端 format） → Task 2/3/4 + Task 6 ✅

**Placeholder scan：** 无 TBD/TODO；所有代码步骤均含完整代码。

**Type consistency：** `ModelPricing` 字段（input/output/cache_read/cache_creation）在 Task 2/3/4 一致；`SyncReport`/`SyncStatus` 字段在 Task 4/5 一致；前端类型字段（snake_case）与 Rust `Serialize` 输出一致；command 名在 Task 5/6 一致。

**已知取舍：** `Summary.total_cost_usd` 与分组统计的金额经 `f64` 中转后 `round_dp(8)`，对个人用量精度足够；如需严格 Decimal 聚合可后续改为 `rust_decimal` SQL 聚合。
