use crate::{models::CopyItem, storage};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
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

pub async fn generate_copy(app: &AppHandle, prompt: &str) -> Result<CopyItem, String> {
    let full_prompt = single_prompt(prompt);
    let text = call_api(app, &full_prompt, 600, 0.9).await?;
    parse_item(&text).ok_or_else(|| "AI 返回内容无法解析，请检查提示词格式".into())
}

fn single_prompt(prompt: &str) -> String {
    format!("{prompt}\n\n严格按以下格式输出，不要添加解释：\n标题：xxx\n正文：xxx\n话题：#xx #xx")
}

pub async fn generate_batch(
    app: &AppHandle,
    prompt: &str,
    count: usize,
) -> Result<Vec<CopyItem>, String> {
    let count = count.clamp(1, 100);
    let full_prompt = format!("{prompt}\n\n生成{count}条差异化内容，每条按【第N条】分隔。每条严格包含：标题：xxx、正文：xxx、话题：#xx #xx");
    let text = call_api(app, &full_prompt, (count as u32 * 600).min(12000), 0.9).await?;
    let items = parse_batch(&text);
    if items.is_empty() {
        return Err("AI 批量返回内容无法解析".into());
    }
    Ok(items)
}

async fn call_api(
    app: &AppHandle,
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
) -> Result<String, String> {
    let app = app.clone();
    let (url, key, model) = tauri::async_runtime::spawn_blocking(move || storage::ai_config(&app))
        .await
        .map_err(|error| format!("读取 API 配置任务失败: {error}"))??;
    if key.is_empty() {
        return Err("请先在设置页配置 API Key".into());
    }
    if prompt.trim().is_empty() {
        return Err("提示词为空".into());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(|_| "创建 AI 客户端失败".to_string())?;
    let response = client
        .post(url)
        .bearer_auth(key)
        .json(&ChatRequest {
            model: &model,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
            max_tokens,
            temperature,
        })
        .send()
        .await
        .map_err(|_| "AI 请求失败或超时".to_string())?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|_| "AI 返回不是有效 JSON".to_string())?;
    if !status.is_success() {
        return Err(format!("AI 接口返回错误（HTTP {}）", status.as_u16()));
    }
    serde_json::from_value::<ChatResponse>(body)
        .map_err(|_| "AI 返回缺少 choices.message.content".into())
        .and_then(|data| {
            data.choices
                .into_iter()
                .next()
                .map(|choice| choice.message.content)
                .ok_or_else(|| "AI 返回为空".into())
        })
}

fn parse_item(text: &str) -> Option<CopyItem> {
    let mut item = CopyItem {
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
    (!item.title.is_empty() && !item.body.is_empty()).then_some(item)
}

fn parse_batch(text: &str) -> Vec<CopyItem> {
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
        let prefix = format!("{field}{separator}");
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some(value.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_copy() {
        let item = parse_item("标题：今晚约球\n正文：第一行\n第二行\n话题：#珠海台球").unwrap();
        assert_eq!(item.title, "今晚约球");
        assert_eq!(item.body, "第一行\n第二行");
        assert_eq!(item.tags, "#珠海台球");
    }

    #[test]
    fn parses_batch_sections() {
        let items = parse_batch(
            "【第1条】\n标题：一\n正文：甲\n话题：#一\n【第2条】\n标题：二\n正文：乙\n话题：#二",
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].title, "二");
    }

    #[test]
    fn single_prompt_requires_parseable_fields() {
        let prompt = single_prompt("生成一条约球文案");
        assert!(prompt.contains("标题：xxx"));
        assert!(prompt.contains("正文：xxx"));
        assert!(prompt.contains("话题：#xx #xx"));
        assert!(prompt.contains("不要添加解释"));
    }
}
