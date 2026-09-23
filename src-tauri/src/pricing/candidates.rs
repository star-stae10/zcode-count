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
