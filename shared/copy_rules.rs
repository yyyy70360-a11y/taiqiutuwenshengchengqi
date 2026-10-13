use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::OnceLock};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct TemplateCapacity {
    pub title: usize,
    pub body: usize,
    pub body_lines: usize,
    pub tags: usize,
    pub tag: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateSpec {
    pub id: String,
    #[allow(dead_code)] // Used by the desktop catalog, not the API service.
    pub name: String,
    #[allow(dead_code)]
    pub category: String,
    pub capacity: TemplateCapacity,
}

pub fn catalog() -> &'static [TemplateSpec] {
    static CATALOG: OnceLock<Vec<TemplateSpec>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("template-catalog.json")).expect("valid template catalog")
    })
}

pub fn template_capacity(template: Option<&str>) -> TemplateCapacity {
    let id = template.unwrap_or("magazine").trim();
    let id = if id == "mono" { "minimal" } else { id };
    catalog()
        .iter()
        .find(|item| item.id == id)
        .unwrap_or(&catalog()[0])
        .capacity
}

pub fn count_chars(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

pub fn required_facts(prompt: &str) -> Vec<String> {
    let Some((_, tail)) = prompt.split_once("【可使用的比赛条件】") else {
        return Vec::new();
    };
    let facts = tail.split('【').next().unwrap_or("").trim();
    if facts.starts_with("未填写") {
        return Vec::new();
    }
    facts
        .split(['，', ',', '。', ';', '；', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect()
}

pub fn critical_clauses(body: &str) -> Vec<String> {
    body.split(['，', ',', '。', ';', '；', '\n', '！', '？'])
        .map(str::trim)
        .filter(|part| {
            !part.is_empty()
                && ["档", "报名费", "台费", "打完", "完赛", "让球"]
                    .iter()
                    .any(|key| part.contains(key))
        })
        .map(String::from)
        .collect()
}

pub fn validate_match_conditions(prompt: &str, body: &str) -> Result<(), String> {
    if !prompt.contains("【可使用的比赛条件】") {
        return Ok(());
    }
    let source = required_facts(prompt).join("；");
    for clause in critical_clauses(body) {
        if ["未确认", "待确认", "另行确认", "还没确认", "不等于"]
            .iter()
            .any(|word| clause.contains(word))
        {
            continue;
        }
        for key in ["报名费", "台费", "让球"] {
            if clause.contains(key)
                && !source.contains(key)
                && !(key == "完赛" && source.contains("打完"))
                && !(key == "打完" && source.contains("完赛"))
            {
                return Err(format!("新增了未填写的比赛条件：{clause}"));
            }
        }
        for free in ["免报名费", "免台费"] {
            if contains_fact(&fact_text(&clause), free) && !contains_fact(&fact_text(&source), free)
            {
                return Err(format!("新增了未填写的免费条件：{clause}"));
            }
        }
        let timed = ["一天", "两天", "三天", "当天", "小时", "分钟"]
            .iter()
            .any(|word| clause.contains(word));
        if timed
            && (clause.contains("打完") || clause.contains("完赛"))
            && !source.contains("打完")
            && !source.contains("完赛")
        {
            return Err(format!("新增了未填写的赛程：{clause}"));
        }
        if clause.contains('限') && clause.contains('档') && !source.contains('档') {
            return Err(format!("新增了未填写的档位限制：{clause}"));
        }
    }
    Ok(())
}

pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn fact_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| {
            if ['–', '—', '－'].contains(&ch) {
                '-'
            } else {
                ch
            }
        })
        .collect()
}
fn contains_fact(content: &str, fact: &str) -> bool {
    content.match_indices(fact).any(|(index, _)| {
        let prefix = &content[..index];
        ![
            "不",
            "不是",
            "并非",
            "无需",
            "不用",
            "不需要",
            "取消",
            "不再",
        ]
        .iter()
        .any(|negative| prefix.ends_with(negative))
    })
}

pub fn validate(
    title: &str,
    body: &str,
    tags: &str,
    cap: TemplateCapacity,
    facts: &[String],
) -> Result<(), String> {
    if title.trim().is_empty() || body.trim().is_empty() {
        return Err("标题或正文为空".into());
    }
    if count_chars(title) > cap.title
        || count_chars(body) > cap.body
        || body.lines().count() > cap.body_lines
    {
        return Err("文案超出模板容量，请压缩表达并保留条件".into());
    }
    let parts: Vec<_> = tags.split_whitespace().collect();
    if parts.len() > cap.tags || parts.iter().any(|part| count_chars(part) > cap.tag) {
        return Err("话题超出模板容量".into());
    }
    let content = fact_text(&format!("{title}\n{body}"));
    for fact in facts {
        if !contains_fact(&content, &fact_text(fact)) {
            return Err(format!("缺少或改动比赛条件：{fact}"));
        }
    }
    if facts.iter().any(|fact| fact.contains("报名费"))
        && (body.contains("全程免费") || body.contains("完全免费") || body.contains("零花费"))
    {
        return Err("报名费用不能扩写为全程免费".into());
    }
    if body.contains('…') {
        return Err("正文包含省略截断，请返回完整短文".into());
    }
    Ok(())
}

pub fn repair_prompt(
    prompt: &str,
    title: &str,
    body: &str,
    tags: &str,
    cap: TemplateCapacity,
    facts: &[String],
    reason: &str,
) -> String {
    format!("{prompt}\n\n【改写当前文案】\n问题：{reason}\n标题：{title}\n正文：{body}\n话题：{tags}\n请缩短表达，不能截断结尾或用省略号，不能添加新事实。以下关键条件逐项原样保留：{}。\n标题≤{}字；正文≤{}字、最多{}行；话题最多{}个、每个≤{}字。只返回标题、正文、话题三字段。", facts.join("；"), cap.title, cap.body, cap.body_lines, cap.tags, cap.tag)
}

fn bigrams(text: &str) -> HashSet<String> {
    let chars: Vec<_> = normalize(text).chars().collect();
    chars.windows(2).map(|pair| pair.iter().collect()).collect()
}

pub fn near_duplicate(title: &str, body: &str, existing_title: &str, existing_body: &str) -> bool {
    let lhs = normalize(&format!("{title}{body}"));
    let rhs = normalize(&format!("{existing_title}{existing_body}"));
    if lhs == rhs || normalize(body) == normalize(existing_body) {
        return true;
    }
    let opening: String = normalize(body).chars().take(14).collect();
    if opening.chars().count() >= 10
        && opening
            == normalize(existing_body)
                .chars()
                .take(14)
                .collect::<String>()
        && normalize(title) == normalize(existing_title)
    {
        return true;
    }
    let a = bigrams(&lhs);
    let b = bigrams(&rhs);
    !a.is_empty()
        && !b.is_empty()
        && (2.0 * a.intersection(&b).count() as f64 / (a.len() + b.len()) as f64) >= 0.86
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_covers_fifty_unique_templates() {
        let ids: HashSet<_> = catalog().iter().map(|item| &item.id).collect();
        assert_eq!(ids.len(), 50);
        assert_eq!(template_capacity(Some("poster")).body, 144);
        assert_eq!(template_capacity(Some("vaporwave")).body, 112);
    }
    #[test]
    fn fees_at_end_are_never_silently_cut() {
        let prompt =
            "【可使用的比赛条件】\n限1–3档，免报名费，败方结台费，一天打完\n【比赛条件与表达要求】";
        let facts = required_facts(prompt);
        let cap = template_capacity(None);
        assert!(validate(
            "找搭子",
            "限1–3档，免报名费，一天打完",
            "#台球",
            cap,
            &facts
        )
        .is_err());
        assert!(validate(
            "找搭子",
            "限1–3档，免报名费，败方结台费，一天打完",
            "#台球",
            cap,
            &facts
        )
        .is_ok());
        assert!(validate("找搭子", &"练球".repeat(80), "", cap, &facts).is_err());
    }
    #[test]
    fn repetition_and_empty_conditions_are_detected() {
        assert!(near_duplicate(
            "先练再去",
            "限1–3档，免报名费，一天打完。有一起去的么？",
            "比赛找搭子",
            "限1–3档，免报名费，一天打完。有一起去的么？"
        ));
        assert!(near_duplicate(
            "一起参赛",
            "有没有想去比赛的球搭子？",
            "一起参赛",
            "有没有想去比赛的球搭子！"
        ));
        assert!(!near_duplicate(
            "先练走位",
            "今晚练练控球再聊比赛",
            "初次参赛",
            "有点紧张想找伴一起去"
        ));
        assert!(required_facts("【可使用的比赛条件】\n未填写。\n【说明】").is_empty());
    }

    #[test]
    fn ranges_and_negative_fee_statements_cannot_change_facts() {
        let cap = template_capacity(None);
        let facts = vec!["限1–3档".into(), "免报名费".into(), "败方结台费".into()];
        assert!(validate("找搭子", "限13档，免报名费，败方结台费", "", cap, &facts).is_err());
        assert!(validate("找搭子", "限1-3档，不免报名费，败方结台费", "", cap, &facts).is_err());
        assert!(validate(
            "找搭子",
            "限1-3档，免报名费，无需败方结台费",
            "",
            cap,
            &facts
        )
        .is_err());
        assert!(validate("找搭子", "限1-3档，免报名费，败方结台费", "", cap, &facts).is_ok());
    }

    #[test]
    fn cleared_conditions_do_not_accept_invented_fees() {
        let empty = "【可使用的比赛条件】\n未填写。\n【说明】";
        assert!(
            validate_match_conditions(empty, "有没有一起去比赛的？免报名费，败方结台费。").is_err()
        );
        assert!(validate_match_conditions(empty, "我2档，想找个同城球友一起了解比赛。").is_ok());
        assert!(validate_match_conditions(empty, "找个伴一起参赛，打完再聊聊。").is_ok());
        assert!(validate_match_conditions(
            "【可使用的比赛条件】\n报名费39元\n【说明】",
            "免报名费找搭子。报名费39元，有想一起去的么？"
        )
        .is_err());
        let filled = "【可使用的比赛条件】\n限1–3档，免报名费，一天打完\n【说明】";
        assert!(
            validate_match_conditions(filled, "限1–3档，免报名费，一天打完，败方结台费。").is_err()
        );
        assert!(
            validate_match_conditions(filled, "限1–3档，免报名费，一天打完，台费待确认。").is_ok()
        );
    }
}
