use crate::{models::CopyItem, storage};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Clone, Copy)]
pub struct TemplateCapacity {
    pub title: usize,
    pub body: usize,
    pub body_lines: usize,
    pub tags: usize,
    pub tag: usize,
}

const STANDARD_CAPACITY_TEMPLATES: &[&str] = &[
    "magazine_pro",
    "fresh",
    "journal",
    "neon_club",
    "chalkboard",
    "retro_ticket",
    "cyber_grid",
    "cream_note",
    "arena_score",
    "sunset_gradient",
    "ink_stamp",
    "glass_card",
    "tactical_blue",
    "midnight_lux",
    "candy_pop",
    "forest_match",
    "steel_gray",
    "royal_gold",
    "ocean_wave",
    "lava_motion",
    "pearl_lite",
    "street_snap",
    "comic_burst",
    "vaporwave",
    "newspaper",
    "coffee_receipt",
    "scoreboard_green",
    "purple_stage",
    "ice_blue",
    "red_warning",
    "kraft_label",
    "mint_mono",
    "black_gold",
    "gradient_ring",
    "billiard_felt",
    "tournament_bracket",
    "soft_shadow",
    "bold_blocks",
    "pink_soda",
    "desert_sand",
    "matrix_code",
    "club_vip",
    "clean_blue",
    "orange_zine",
    "silver_card",
    "green_laser",
    "classic_serif",
];

pub fn template_capacity(template: Option<&str>) -> TemplateCapacity {
    let template = template.unwrap_or("magazine").trim();
    if STANDARD_CAPACITY_TEMPLATES.contains(&template) {
        return TemplateCapacity {
            title: 30,
            body: 112,
            body_lines: 7,
            tags: 3,
            tag: 12,
        };
    }
    match template {
        "minimal" | "mono" => TemplateCapacity {
            title: 30,
            body: 136,
            body_lines: 8,
            tags: 3,
            tag: 12,
        },
        "poster" => TemplateCapacity {
            title: 30,
            body: 144,
            body_lines: 8,
            tags: 3,
            tag: 12,
        },
        _ => TemplateCapacity {
            title: 30,
            body: 96,
            body_lines: 6,
            tags: 3,
            tag: 12,
        },
    }
}

pub fn fit_item_to_capacity(mut item: CopyItem, capacity: TemplateCapacity) -> CopyItem {
    item.title = trim_chars(&item.title, capacity.title);
    item.body = fit_body(&item.body, capacity);
    item.tags = item
        .tags
        .split_whitespace()
        .take(capacity.tags)
        .map(|tag| trim_chars(tag, capacity.tag))
        .collect::<Vec<_>>()
        .join(" ");
    item
}

fn fit_body(body: &str, capacity: TemplateCapacity) -> String {
    let per_line = capacity.body.div_ceil(capacity.body_lines).max(1);
    let mut lines = Vec::new();
    let mut truncated = count_chars(body) > capacity.body;
    for raw in body.trim().lines() {
        let chars: Vec<char> = raw.trim().chars().collect();
        if chars.is_empty() {
            continue;
        }
        for chunk in chars.chunks(per_line) {
            lines.push(chunk.iter().collect::<String>());
        }
    }
    if lines.len() > capacity.body_lines {
        truncated = true;
        lines.truncate(capacity.body_lines);
    }
    let mut fitted = lines.join("\n");
    fitted = trim_chars(&fitted, capacity.body);
    if truncated && !fitted.ends_with('…') {
        fitted = trim_chars(&fitted, capacity.body.saturating_sub(1));
        fitted.push('…');
    }
    fitted
}

fn trim_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.trim().chars();
    let mut result = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() && max_chars > 0 {
        result.pop();
        result.push('…');
    }
    result
}

fn count_chars(value: &str) -> usize {
    value.trim().chars().count()
}

pub fn single_prompt(prompt: &str, capacity: TemplateCapacity) -> String {
    format!(
        "{prompt}\n\n{limits}\n严格按以下格式输出，不要添加解释：\n标题：xxx\n正文：xxx\n话题：#xx #xx",
        limits = capacity_instruction(capacity)
    )
}

pub fn batch_prompt(prompt: &str, count: usize, capacity: TemplateCapacity) -> String {
    format!(
        "{prompt}\n\n{limits}\n生成{count}条差异化内容，每条按【第N条】分隔。每条严格包含：标题：xxx、正文：xxx、话题：#xx #xx",
        limits = capacity_instruction(capacity)
    )
}

fn capacity_instruction(capacity: TemplateCapacity) -> String {
    format!(
        "【模板容量限制：最高优先级】标题≤{}字；正文≤{}字且最多{}行；话题最多{}个，每个≤{}字。宁可短一点，不要撑满或超出模板。",
        capacity.title, capacity.body, capacity.body_lines, capacity.tags, capacity.tag
    )
}

pub fn parse_item(text: &str) -> Option<CopyItem> {
    let mut item = CopyItem {
        cloud_id: None,
        title: String::new(),
        body: String::new(),
        tags: String::new(),
    };
    let mut mode = "";
    for line in text.lines().map(str::trim) {
        if let Some(value) = strip_field(line, "标题") {
            item.title = value;
            mode = "";
        } else if let Some(value) = strip_field(line, "正文") {
            item.body = value;
            mode = "body";
        } else if let Some(value) = strip_field(line, "话题") {
            item.tags = value;
            mode = "";
        } else if mode == "body" && !line.is_empty() {
            if !item.body.is_empty() {
                item.body.push('\n');
            }
            item.body.push_str(line);
        }
    }
    if !item.title.is_empty() && !item.body.is_empty() {
        Some(item)
    } else {
        None
    }
}

pub fn parse_batch(text: &str) -> Vec<CopyItem> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let normalized = line.trim();
        if (normalized.starts_with('【') && normalized.contains('第'))
            || (normalized.starts_with('[') && normalized.contains('第'))
        {
            if !current.trim().is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
        .iter()
        .filter_map(|chunk| parse_item(chunk))
        .collect()
}

fn strip_field(line: &str, field: &str) -> Option<String> {
    for separator in ['：', ':'] {
        if let Some(value) = line.strip_prefix(&format!("{field}{separator}")) {
            return Some(value.trim().to_string());
        }
    }
    None
}

pub fn normalize_chat_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

pub async fn call_chat_completions(
    client: &Client,
    endpoint: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    timeout_secs: u64,
) -> Result<String, String> {
    let response = client
        .post(endpoint)
        .bearer_auth(api_key.trim())
        .timeout(Duration::from_secs(timeout_secs))
        .json(&ChatRequest {
            model: model.trim(),
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
            max_tokens,
            temperature: 0.9,
        })
        .send()
        .await
        .map_err(|err| format!("AI 接口请求超时或网络不可用: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 => "API Key 鉴权失败 (HTTP 401)，请检查输入的 API Key 是否有效".into(),
            404 => format!("接口地址或模型不存在 (HTTP 404，请求路径: {endpoint}，模型: {model})"),
            429 => "API 请求频率超限或账户余额不足 (HTTP 429)".into(),
            _ => format!("AI 服务返回错误 (HTTP {}): {}", status.as_u16(), err_body),
        });
    }

    let body: Value = response
        .json()
        .await
        .map_err(|err| format!("AI 返回内容不是有效 JSON: {err}"))?;

    serde_json::from_value::<ChatResponse>(body)
        .map_err(|_| "AI 返回缺少 choices 或 message 字段".to_string())?
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| "AI 返回内容为空".to_string())
}

pub async fn test_connection(
    app: &AppHandle,
    url: Option<&str>,
    model: Option<&str>,
    api_key: Option<&str>,
) -> Result<String, String> {
    let settings = storage::get_settings(app)?;
    let base_url = url
        .filter(|u| !u.trim().is_empty())
        .map(String::from)
        .or_else(|| settings.get("api_url").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "https://api.deepseek.com/v1".into());

    let model_name = model
        .filter(|m| !m.trim().is_empty())
        .map(String::from)
        .or_else(|| settings.get("api_model").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "deepseek-chat".into());

    let key = match api_key.filter(|k| !k.trim().is_empty()) {
        Some(k) => k.to_string(),
        None => storage::read_api_key()?
            .ok_or_else(|| "未填写且未保存 API Key，请输入 API Key 后再测试".to_string())?,
    };

    let endpoint = normalize_chat_endpoint(&base_url);
    let client = Client::builder()
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;

    let started = Instant::now();
    let reply = call_chat_completions(
        &client,
        &endpoint,
        &key,
        &model_name,
        "请回复：pong",
        20,
        15,
    )
    .await?;

    let elapsed = started.elapsed().as_millis();
    Ok(format!(
        "连接成功！模型 [{}] 响应正常（耗时 {}ms）\n返回内容：{}",
        model_name,
        elapsed,
        reply.trim()
    ))
}

pub async fn generate_copy(
    app: &AppHandle,
    prompt: &str,
    template: Option<&str>,
) -> Result<CopyItem, String> {
    if prompt.trim().is_empty() {
        return Err("提示词为空".into());
    }

    let settings = storage::get_settings(app)?;
    let base_url = settings
        .get("api_url")
        .and_then(Value::as_str)
        .unwrap_or("https://api.deepseek.com/v1");
    let model = settings
        .get("api_model")
        .and_then(Value::as_str)
        .unwrap_or("deepseek-chat");

    let api_key = storage::read_api_key()?
        .ok_or_else(|| "尚未配置本地 API Key，请先在“设置”页中填写 API Key 并保存".to_string())?;

    let capacity = template_capacity(template);
    let endpoint = normalize_chat_endpoint(base_url);
    let client = Client::builder()
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;

    let formatted_prompt = single_prompt(prompt, capacity);
    let text = call_chat_completions(
        &client,
        &endpoint,
        &api_key,
        model,
        &formatted_prompt,
        600,
        45,
    )
    .await?;

    let item = parse_item(&text)
        .map(|item| fit_item_to_capacity(item, capacity))
        .ok_or_else(|| format!("AI 返回内容无法解析为标题与正文：\n{text}"))?;

    Ok(item)
}

pub async fn generate_batch(
    app: &AppHandle,
    prompt: &str,
    count: usize,
    template: Option<&str>,
) -> Result<Vec<CopyItem>, String> {
    if prompt.trim().is_empty() {
        return Err("提示词为空".into());
    }
    let count = count.clamp(1, 100);

    let settings = storage::get_settings(app)?;
    let base_url = settings
        .get("api_url")
        .and_then(Value::as_str)
        .unwrap_or("https://api.deepseek.com/v1");
    let model = settings
        .get("api_model")
        .and_then(Value::as_str)
        .unwrap_or("deepseek-chat");

    let api_key = storage::read_api_key()?
        .ok_or_else(|| "尚未配置本地 API Key，请先在“设置”页中填写 API Key 并保存".to_string())?;

    let capacity = template_capacity(template);
    let endpoint = normalize_chat_endpoint(base_url);
    let client = Client::builder()
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败: {err}"))?;

    let formatted_prompt = batch_prompt(prompt, count, capacity);
    let max_tokens = (count as u32 * 600).min(12000);
    let timeout_secs = (30 + count as u64 * 3).min(120);

    let text = call_chat_completions(
        &client,
        &endpoint,
        &api_key,
        model,
        &formatted_prompt,
        max_tokens,
        timeout_secs,
    )
    .await?;

    let items: Vec<CopyItem> = parse_batch(&text)
        .into_iter()
        .map(|item| fit_item_to_capacity(item, capacity))
        .collect();

    if items.is_empty() {
        return Err(format!("AI 批量返回内容无法解析为【第N条】格式：\n{text}"));
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_endpoint_correctly() {
        assert_eq!(
            normalize_chat_endpoint("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_endpoint("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            normalize_chat_endpoint("https://api.deepseek.com/v1/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn parses_single_copy_item() {
        let raw = "标题：今晚打球走起\n正文：约了几个球友\n一起切磋技术\n话题：#台球 #约球";
        let item = parse_item(raw).expect("should parse");
        assert_eq!(item.title, "今晚打球走起");
        assert_eq!(item.body, "约了几个球友\n一起切磋技术");
        assert_eq!(item.tags, "#台球 #约球");
    }

    #[test]
    fn parses_batch_copy_items() {
        let raw = "【第1条】\n标题：第一局\n正文：精彩对决\n话题：#台球\n【第2条】\n标题：第二局\n正文：翻盘胜利\n话题：#技术";
        let items = parse_batch(raw);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "第一局");
        assert_eq!(items[1].title, "第二局");
    }

    #[test]
    fn fits_within_capacity() {
        let cap = template_capacity(Some("magazine"));
        let item = CopyItem {
            cloud_id: None,
            title: "超长标题测试超过了三十个字符的部分应该被自动截断并且添加省略符号".into(),
            body: "超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试超长正文测试".into(),
            tags: "#标签一 #标签二 #标签三 #标签四".into(),
        };
        let fitted = fit_item_to_capacity(item, cap);
        assert!(fitted.title.chars().count() <= cap.title);
        assert!(fitted.body.chars().count() <= cap.body + 1);
        assert_eq!(fitted.tags.split_whitespace().count(), 3);
    }
}

