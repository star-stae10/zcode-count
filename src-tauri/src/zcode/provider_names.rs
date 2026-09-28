//! ZCode provider 配置中的供应商名称映射（Issue 修复 P1-A）。
//!
//! 名称来源：`%USERPROFILE%\.zcode\v2\provider_config.json` 的
//! `config.providerConfigRules.providerRules[]`，每条含 `providerId`、
//! `providerName`、可含 `config.api.baseUrl`。
//!
//! 每次 sync 时把解析结果 upsert 进自有库 `provider_names` 表（只增不删）：
//! provider 一旦从 ZCode 配置中删除/重建，历史 ID 仍保留同步时捕获的名称。

use crate::error::AppError;
use std::path::{Path, PathBuf};

/// 从 ZCode provider 配置解析出的一条名称映射（供内部传递给 sync upsert）。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderName {
    pub provider_id: String,
    pub display_name: String,
    pub base_url: Option<String>,
}

/// provider 配置路径：%USERPROFILE%\.zcode\v2\provider_config.json（回退 HOME）。
pub fn provider_config_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".zcode").join("v2").join("provider_config.json")
}

/// 解析 provider 配置。
///
/// - 文件缺失 → `Ok(vec![])`（视为空映射，不阻断 sync）；
/// - JSON 损坏 → `Err`（由调用方决定降级）；
/// - 条目缺 `providerId` 或 `providerName`（或为空白）→ 跳过该条目。
pub fn load(path: &Path) -> Result<Vec<ProviderName>, AppError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| AppError::Config(format!("provider 配置 JSON 解析失败: {e}")))?;
    let rules = v
        .get("config")
        .and_then(|c| c.get("providerConfigRules"))
        .and_then(|r| r.get("providerRules"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for rule in &rules {
        let pid = rule.get("providerId").and_then(|x| x.as_str()).map(str::trim);
        let name = rule.get("providerName").and_then(|x| x.as_str()).map(str::trim);
        let (Some(pid), Some(name)) = (pid, name) else {
            continue; // 缺 providerId / providerName → 跳过
        };
        if pid.is_empty() || name.is_empty() {
            continue;
        }
        let base_url = rule
            .get("config")
            .and_then(|c| c.get("api"))
            .and_then(|a| a.get("baseUrl"))
            .and_then(|b| b.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        out.push(ProviderName {
            provider_id: pid.to_string(),
            display_name: name.to_string(),
            base_url,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("zc-pname-{}-{tag}.json", std::process::id()))
    }

    fn write(tag: &str, content: &str) -> PathBuf {
        let p = temp_path(tag);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn load_parses_rules_with_base_url() {
        let p = write(
            "ok",
            r#"{
              "schemaVersion": 1,
              "config": {
                "providerConfigRules": {
                  "providerRules": [
                    {
                      "providerId": "opencode-go-chat",
                      "providerName": "OpenCode Go (Chat)",
                      "config": { "api": { "baseUrl": "https://opencode.ai/zen/go/v1" } }
                    },
                    {
                      "providerId": "qwen-cn",
                      "providerName": "阿里云百炼（中国）"
                    }
                  ]
                }
              }
            }"#,
        );
        let names = load(&p).unwrap();
        assert_eq!(names.len(), 2, "两条规则都应解析");
        assert_eq!(names[0].provider_id, "opencode-go-chat");
        assert_eq!(names[0].display_name, "OpenCode Go (Chat)");
        assert_eq!(names[0].base_url.as_deref(), Some("https://opencode.ai/zen/go/v1"));
        assert_eq!(names[1].provider_id, "qwen-cn");
        assert_eq!(names[1].base_url, None, "无 baseUrl 的条目应为 None");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn load_missing_file_returns_empty_ok() {
        let p = std::env::temp_dir().join("zc-pname-definitely-missing.json");
        let _ = std::fs::remove_file(&p);
        let names = load(&p).unwrap();
        assert!(names.is_empty(), "文件缺失应视为空映射且 Ok");
    }

    #[test]
    fn load_broken_json_is_err() {
        let p = write("broken", "{ not valid json !!!");
        let err = load(&p).unwrap_err();
        assert!(err.to_string().contains("JSON"), "错误信息应说明 JSON 问题: {err}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn load_skips_entries_without_provider_id_or_name() {
        let p = write(
            "skip",
            r#"{
              "config": {
                "providerConfigRules": {
                  "providerRules": [
                    { "providerId": "only-id" },
                    { "providerName": "only-name" },
                    { "providerId": "", "providerName": "empty id" },
                    { "providerId": "ok-id", "providerName": "  " },
                    { "providerId": "good", "providerName": "Good Name" }
                  ]
                }
              }
            }"#,
        );
        let names = load(&p).unwrap();
        assert_eq!(names.len(), 1, "缺字段/空白字段条目应被跳过: {names:?}");
        assert_eq!(names[0].provider_id, "good");
        assert_eq!(names[0].display_name, "Good Name");
        let _ = std::fs::remove_file(&p);
    }

    /// 结构缺失（无 providerRules / 空 JSON 对象）→ 空列表、不报错。
    #[test]
    fn load_tolerates_missing_structure() {
        let p = write("empty-obj", "{}");
        assert!(load(&p).unwrap().is_empty());
        let p2 = write("no-rules", r#"{"config": {"providerOrder": []}}"#);
        assert!(load(&p2).unwrap().is_empty());
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&p2);
    }
}
