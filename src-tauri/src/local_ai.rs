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

use crate::copy_rules::{self, template_capacity, TemplateCapacity};

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
        .or_else(|| {
            settings
                .get("api_url")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| "https://api.deepseek.com/v1".into());

    let model_name = model
        .filter(|m| !m.trim().is_empty())
        .map(String::from)
        .or_else(|| {
            settings
                .get("api_model")
                .and_then(Value::as_str)
                .map(String::from)
        })
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

    let item =
        parse_item(&text).ok_or_else(|| format!("AI 返回内容无法解析为标题与正文：\n{text}"))?;
    let provider = Provider {
        client: &client,
        endpoint: &endpoint,
        api_key: &api_key,
        model,
    };
    validate_or_rewrite(&provider, prompt, item, capacity).await
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

    let provider = Provider {
        client: &client,
        endpoint: &endpoint,
        api_key: &api_key,
        model,
    };
    let mut items = Vec::new();
    let mut last_reason = String::new();
    for item in parse_batch(&text).into_iter().take(count) {
        match validate_or_rewrite(&provider, prompt, item, capacity).await {
            Ok(item) => items.push(item),
            Err(reason) => last_reason = reason,
        }
    }

    if items.is_empty() {
        if !last_reason.is_empty() {
            return Err(format!("本组没有合格文案：{last_reason}"));
        }
        return Err(format!("AI 批量返回内容无法解析为【第N条】格式：\n{text}"));
    }

    Ok(items)
}

struct Provider<'a> {
    client: &'a Client,
    endpoint: &'a str,
    api_key: &'a str,
    model: &'a str,
}

async fn validate_or_rewrite(
    provider: &Provider<'_>,
    prompt: &str,
    mut item: CopyItem,
    cap: TemplateCapacity,
) -> Result<CopyItem, String> {
    let mut facts = copy_rules::required_facts(prompt);
    if facts.is_empty() && !prompt.contains("【可使用的比赛条件】") {
        facts = copy_rules::critical_clauses(&item.body);
    }
    for attempt in 0..=2 {
        let reason = match copy_rules::validate(&item.title, &item.body, &item.tags, cap, &facts)
            .and_then(|_| {
                copy_rules::validate_match_conditions(
                    prompt,
                    &format!("{}。{}", item.title, item.body),
                )
            }) {
            Ok(()) => return Ok(item),
            Err(reason) => reason,
        };
        if attempt == 2 {
            return Err(format!(
                "文案改写后仍不合格：{reason}。原始条件没有被截断，请缩短条件或换容量更大的模板。"
            ));
        }
        let repair = copy_rules::repair_prompt(
            prompt,
            &item.title,
            &item.body,
            &item.tags,
            cap,
            &facts,
            &reason,
        );
        let text = call_chat_completions(
            provider.client,
            provider.endpoint,
            provider.api_key,
            provider.model,
            &repair,
            700,
            45,
        )
        .await?;
        item = parse_item(&text).ok_or_else(|| "改写结果缺少标题或正文".to_string())?;
    }
    unreachable!()
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
    fn over_capacity_copy_is_rejected_instead_of_truncated() {
        let cap = template_capacity(Some("magazine"));
        assert!(copy_rules::validate("找搭子", &"练球".repeat(100), "#台球", cap, &[]).is_err());
    }

    #[tokio::test]
    async fn rewrite_calls_provider_and_preserves_fee_at_end() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(position) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..position]);
                    let len: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= position + 4 + len {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.contains("败方结台费"));
            assert!(request.contains("不能截断结尾"));
            let body=serde_json::json!({"choices":[{"message":{"content":"标题：找个参赛搭子\n正文：限1–3档，免报名费，败方结台费，一天打完。有没有一起去的？\n话题：#台球"}}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let client = Client::new();
        let provider = Provider {
            client: &client,
            endpoint: &endpoint,
            api_key: "test-only",
            model: "fixture",
        };
        let original = CopyItem {
            cloud_id: None,
            title: "找搭子".into(),
            body: format!("{}败方结台费", "想一起练球".repeat(30)),
            tags: "#台球".into(),
        };
        let prompt = "【可使用的比赛条件】\n限1–3档，免报名费，败方结台费，一天打完\n【说明】";
        let item = validate_or_rewrite(&provider, prompt, original, template_capacity(None))
            .await
            .unwrap();
        assert!(item.body.contains("败方结台费"));
        assert!(!item.body.contains('…'));
        thread.join().unwrap();
    }
}
