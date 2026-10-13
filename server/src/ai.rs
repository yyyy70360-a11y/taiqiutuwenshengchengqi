use crate::{
    errors::{ApiError, ApiResult},
    models::{AiRequest, BatchAiRequest, CopyItem, UsageResponse},
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;
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
    let _permit = acquire_ai_permit(&state).await?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request(
            "提示词不能为空且不能超过 20000 个字符",
        ));
    }
    let capacity = template_capacity(input.template.as_deref());
    let started = Instant::now();
    let result = async {
        let text = call_provider(&state, &single_prompt(&input.prompt, capacity), 600).await?;
        let item = parse_item(&text).ok_or_else(|| ApiError::internal("AI 返回内容无法解析"))?;
        validate_or_rewrite(&state, &input.prompt, item, capacity).await
    }
    .await;
    match result {
        Ok(item) => {
            record_usage(
                &state,
                &user_id,
                "generate_copy",
                1,
                "success",
                "",
                started.elapsed().as_millis() as i64,
            )
            .await;
            Ok(Json(item))
        }
        Err(error) => {
            record_usage(
                &state,
                &user_id,
                "generate_copy",
                1,
                "failed",
                error.code,
                started.elapsed().as_millis() as i64,
            )
            .await;
            Err(error)
        }
    }
}

pub async fn generate_batch_copy(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<BatchAiRequest>,
) -> ApiResult<Vec<CopyItem>> {
    let user_id = crate::auth::authenticate(&state.db, &headers).await?;
    let _permit = acquire_ai_permit(&state).await?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request(
            "提示词不能为空且不能超过 20000 个字符",
        ));
    }
    let count = input.count.clamp(1, 100);
    let capacity = template_capacity(input.template.as_deref());
    let started = Instant::now();
    let result = async {
        let mut items: Vec<CopyItem> = Vec::new();
        let mut rounds = 0;
        while items.len() < count && rounds < count.div_ceil(5) * 3 + 3 {
            rounds += 1;
            let remaining = (count - items.len()).min(5);
            let avoid: Vec<_> = items.iter().map(|item| item.title.as_str()).collect();
            let prompt = batch_prompt(
                &format!(
                    "{}\n以下标题已经生成，请换开头和角度：{}",
                    input.prompt,
                    avoid.join("；")
                ),
                remaining,
                capacity,
            );
            let text = call_provider(&state, &prompt, remaining as u32 * 700).await?;
            for item in parse_batch(&text).into_iter().take(remaining) {
                if let Ok(item) = validate_or_rewrite(&state, &input.prompt, item, capacity).await {
                    if !items.iter().any(|old| {
                        copy_rules::near_duplicate(&item.title, &item.body, &old.title, &old.body)
                    }) {
                        items.push(item);
                    }
                }
            }
        }
        if items.len() != count {
            return Err(ApiError::unavailable(format!(
                "只生成 {} / {count} 条合格且不重复文案，请重试缺失部分",
                items.len()
            )));
        }
        Ok(items)
    }
    .await;
    match result {
        Ok(items) => {
            record_usage(
                &state,
                &user_id,
                "generate_batch_copy",
                items.len(),
                "success",
                "",
                started.elapsed().as_millis() as i64,
            )
            .await;
            Ok(Json(items))
        }
        Err(error) => {
            record_usage(
                &state,
                &user_id,
                "generate_batch_copy",
                count,
                "failed",
                error.code,
                started.elapsed().as_millis() as i64,
            )
            .await;
            Err(error)
        }
    }
}

async fn call_provider(
    state: &crate::AppState,
    prompt: &str,
    max_tokens: u32,
) -> Result<String, ApiError> {
    let ai_config = crate::ai_config::load(&state.db, &state.config)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "failed to load AI runtime config");
            ApiError::internal("读取 AI 配置失败")
        })?;
    if ai_config.api_key.trim().is_empty() {
        return Err(ApiError::unavailable("服务器尚未配置 AI Provider"));
    }
    let endpoint = if ai_config.base_url.ends_with("/chat/completions") {
        ai_config.base_url.clone()
    } else {
        format!(
            "{}/chat/completions",
            ai_config.base_url.trim_end_matches('/')
        )
    };
    let response = state
        .ai_client
        .post(endpoint)
        .bearer_auth(&ai_config.api_key)
        .timeout(Duration::from_secs(ai_config.timeout_seconds))
        .json(&ChatRequest {
            model: &ai_config.model,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
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
        return Err(ApiError::unavailable(format!(
            "AI 服务返回错误（HTTP {}）",
            status.as_u16()
        )));
    }
    serde_json::from_value::<ChatResponse>(body)
        .map_err(|_| ApiError::unavailable("AI 返回缺少有效内容"))?
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| ApiError::unavailable("AI 返回为空"))
}

async fn acquire_ai_permit(state: &crate::AppState) -> Result<OwnedSemaphorePermit, ApiError> {
    let semaphore = state.ai_semaphore.read().await.clone();
    semaphore
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("AI 任务繁忙，请稍后重试"))
}

async fn record_usage(
    state: &crate::AppState,
    user_id: &str,
    operation: &str,
    count: usize,
    status: &str,
    error_message: &str,
    duration_ms: i64,
) {
    if let Err(error) = sqlx::query("INSERT INTO usage_records (id, user_id, operation, item_count, status, error_message, duration_ms) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(operation)
        .bind(count as i32)
        .bind(status)
        .bind(error_message)
        .bind(duration_ms)
        .execute(&state.db)
        .await
    {
        tracing::warn!(error = %error, "failed to record AI usage");
    }
}

use crate::copy_rules::{self, template_capacity, TemplateCapacity};

async fn validate_or_rewrite(
    state: &crate::AppState,
    prompt: &str,
    mut item: CopyItem,
    cap: TemplateCapacity,
) -> Result<CopyItem, ApiError> {
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
            return Err(ApiError::bad_request(format!(
                "文案改写后仍不合格：{reason}。请缩短条件或换容量更大的模板。"
            )));
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
        let text = call_provider(state, &repair, 700).await?;
        item = parse_item(&text).ok_or_else(|| ApiError::internal("改写结果缺少标题或正文"))?;
    }
    unreachable!()
}

fn single_prompt(prompt: &str, capacity: TemplateCapacity) -> String {
    format!(
        "{prompt}\n\n{limits}\n严格按以下格式输出，不要添加解释：\n标题：xxx\n正文：xxx\n话题：#xx #xx",
        limits = capacity_instruction(capacity)
    )
}

fn batch_prompt(prompt: &str, count: usize, capacity: TemplateCapacity) -> String {
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

fn parse_item(text: &str) -> Option<CopyItem> {
    let mut item = CopyItem {
        id: None,
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
        if let Some(value) = line.strip_prefix(&format!("{field}{separator}")) {
            return Some(value.trim().to_string());
        }
    }
    None
}

#[allow(dead_code)]
fn _usage_response(operation: String, item_count: usize) -> UsageResponse {
    UsageResponse {
        operation,
        item_count,
    }
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

    #[test]
    fn template_capacity_defaults_to_magazine() {
        let default_capacity = template_capacity(None);
        let magazine_capacity = template_capacity(Some("magazine"));
        assert_eq!(default_capacity.title, magazine_capacity.title);
        assert_eq!(default_capacity.body, 96);
        assert_eq!(default_capacity.body_lines, 6);
        assert_eq!(default_capacity.tags, 3);
        assert_eq!(default_capacity.tag, 12);
    }

    #[test]
    fn poster_capacity_allows_more_body_than_magazine() {
        let magazine_capacity = template_capacity(Some("magazine"));
        let poster_capacity = template_capacity(Some("poster"));
        assert!(poster_capacity.body > magazine_capacity.body);
        assert!(poster_capacity.body_lines > magazine_capacity.body_lines);
    }

    #[test]
    fn new_template_capacities_are_recognized() {
        for template in copy_rules::catalog()
            .iter()
            .filter(|item| !["magazine", "minimal", "poster"].contains(&item.id.as_str()))
            .map(|item| item.id.as_str())
        {
            let capacity = template_capacity(Some(template));
            assert_eq!(capacity.body, 112);
            assert_eq!(capacity.body_lines, 7);
        }
        for template in ["newspaper", "vaporwave", "mono"] {
            let capacity = template_capacity(Some(template));
            let expected = if template == "mono" { 136 } else { 112 };
            assert_eq!(capacity.body, expected);
            assert_eq!(capacity.body_lines, if template == "mono" { 8 } else { 7 });
        }
    }

    #[test]
    fn server_prompts_include_template_capacity_limits() {
        let capacity = template_capacity(Some("minimal"));
        let single = single_prompt("斗门今晚约球", capacity);
        assert!(single.contains("模板容量限制"));
        assert!(single.contains("标题≤30字"));
        assert!(single.contains("正文≤136字且最多8行"));
        assert!(single.contains("话题最多3个，每个≤12字"));

        let batch = batch_prompt("斗门今晚约球", 10, capacity);
        assert!(batch.contains("生成10条差异化内容"));
        assert!(batch.contains("正文≤136字且最多8行"));
    }

    #[test]
    fn oversize_copy_is_rejected_without_losing_fees() {
        let facts = vec!["败方结台费".to_string()];
        assert!(copy_rules::validate(
            "一起去",
            &format!("{}败方结台费", "练球".repeat(90)),
            "",
            template_capacity(None),
            &facts
        )
        .is_err());
    }
}
