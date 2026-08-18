use crate::{errors::{ApiError, ApiResult}, models::{AiRequest, BatchAiRequest, CopyItem, UsageResponse}};
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use uuid::Uuid;

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

pub async fn generate_copy(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<AiRequest>,
) -> ApiResult<CopyItem> {
    let user_id = crate::auth::authenticate(&state.db, &headers).await?;
    let _permit = state.ai_semaphore.clone().try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("AI 任务繁忙，请稍后重试"))?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request("提示词不能为空且不能超过 20000 个字符"));
    }
    let started = Instant::now();
    let text = call_provider(&state, &single_prompt(&input.prompt), 600).await?;
    let item = parse_item(&text).ok_or_else(|| ApiError::internal("AI 返回内容无法解析"))?;
    record_usage(&state, &user_id, "generate_copy", 1, started.elapsed().as_millis() as i64).await;
    Ok(Json(item))
}

pub async fn generate_batch_copy(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<BatchAiRequest>,
) -> ApiResult<Vec<CopyItem>> {
    let user_id = crate::auth::authenticate(&state.db, &headers).await?;
    let _permit = state.ai_semaphore.clone().try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("AI 任务繁忙，请稍后重试"))?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request("提示词不能为空且不能超过 20000 个字符"));
    }
    let count = input.count.clamp(1, 100);
    let started = Instant::now();
    let prompt = format!("{}\n\n生成{}条差异化内容，每条按【第N条】分隔。每条严格包含：标题：xxx、正文：xxx、话题：#xx #xx", input.prompt, count);
    let text = call_provider(&state, &prompt, (count as u32 * 600).min(12000)).await?;
    let items = parse_batch(&text);
    if items.is_empty() {
        return Err(ApiError::internal("AI 批量返回内容无法解析"));
    }
    record_usage(&state, &user_id, "generate_batch_copy", items.len(), started.elapsed().as_millis() as i64).await;
    Ok(Json(items))
}

async fn call_provider(state: &crate::AppState, prompt: &str, max_tokens: u32) -> Result<String, ApiError> {
    if state.config.ai_api_key.trim().is_empty() {
        return Err(ApiError::unavailable("服务器尚未配置 AI Provider"));
    }
    let endpoint = if state.config.ai_base_url.ends_with("/chat/completions") {
        state.config.ai_base_url.clone()
    } else {
        format!("{}/chat/completions", state.config.ai_base_url.trim_end_matches('/'))
    };
    let response = state.ai_client
        .post(endpoint)
        .bearer_auth(&state.config.ai_api_key)
        .json(&ChatRequest {
            model: &state.config.ai_model,
            messages: vec![Message { role: "user", content: prompt }],
            max_tokens,
            temperature: 0.9,
        })
        .send()
        .await
        .map_err(|_| ApiError::unavailable("AI 请求超时或网络不可用"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|_| ApiError::unavailable("AI 返回不是有效 JSON"))?;
    if !status.is_success() {
        tracing::warn!(status = status.as_u16(), "AI provider returned an error");
        return Err(ApiError::unavailable(format!("AI 服务返回错误（HTTP {}）", status.as_u16())));
    }
    serde_json::from_value::<ChatResponse>(body)
        .map_err(|_| ApiError::unavailable("AI 返回缺少有效内容"))?
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| ApiError::unavailable("AI 返回为空"))
}

async fn record_usage(state: &crate::AppState, user_id: &str, operation: &str, count: usize, duration_ms: i64) {
    if let Err(error) = sqlx::query("INSERT INTO usage_records (id, user_id, operation, item_count, status, duration_ms) VALUES ($1, $2, $3, $4, 'success', $5)")
        .bind(Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(operation)
        .bind(count as i32)
        .bind(duration_ms)
        .execute(&state.db)
        .await
    {
        tracing::warn!(error = %error, "failed to record AI usage");
    }
}

fn single_prompt(prompt: &str) -> String {
    format!("{prompt}\n\n严格按以下格式输出，不要添加解释：\n标题：xxx\n正文：xxx\n话题：#xx #xx")
}

fn parse_item(text: &str) -> Option<CopyItem> {
    let mut item = CopyItem { id: None, title: String::new(), body: String::new(), tags: String::new() };
    let mut mode = "";
    for line in text.lines().map(str::trim) {
        if let Some(value) = strip_field(line, "标题") { item.title = value; mode = ""; }
        else if let Some(value) = strip_field(line, "正文") { item.body = value; mode = "body"; }
        else if let Some(value) = strip_field(line, "话题") { item.tags = value; mode = ""; }
        else if mode == "body" && !line.is_empty() { if !item.body.is_empty() { item.body.push('\n'); } item.body.push_str(line); }
    }
    (!item.title.is_empty() && !item.body.is_empty()).then_some(item)
}

fn parse_batch(text: &str) -> Vec<CopyItem> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let normalized = line.trim();
        if (normalized.starts_with('【') && normalized.contains('第')) || (normalized.starts_with('[') && normalized.contains('第')) {
            if !current.trim().is_empty() { chunks.push(std::mem::take(&mut current)); }
        } else { current.push_str(line); current.push('\n'); }
    }
    if !current.trim().is_empty() { chunks.push(current); }
    chunks.iter().filter_map(|chunk| parse_item(chunk)).collect()
}

fn strip_field(line: &str, field: &str) -> Option<String> {
    for separator in ['：', ':'] {
        if let Some(value) = line.strip_prefix(&format!("{field}{separator}")) { return Some(value.trim().to_string()); }
    }
    None
}

#[allow(dead_code)]
fn _usage_response(operation: String, item_count: usize) -> UsageResponse {
    UsageResponse { operation, item_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_copy() {
        let item = parse_item("标题：今晚约球\n正文：第一行\n第二行\n话题：#珠海台球")
            .expect("copy should parse");
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
}
