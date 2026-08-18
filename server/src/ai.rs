use crate::{
    errors::{ApiError, ApiResult},
    models::{AiRequest, BatchAiRequest, CopyItem, UsageResponse},
};
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

#[derive(Debug, Clone, Copy)]
struct CopyLimits {
    title_chars: usize,
    body_chars: usize,
    body_lines: usize,
    tags_count: usize,
    tag_chars: usize,
}

fn limits_for_template(template: Option<&str>) -> CopyLimits {
    match template.unwrap_or("magazine").trim() {
        "minimal" => CopyLimits {
            title_chars: 30,
            body_chars: 136,
            body_lines: 8,
            tags_count: 3,
            tag_chars: 12,
        },
        "poster" => CopyLimits {
            title_chars: 30,
            body_chars: 144,
            body_lines: 8,
            tags_count: 3,
            tag_chars: 12,
        },
        "magazine_pro" | "fresh" | "journal" | "neon_club" | "chalkboard" | "retro_ticket"
        | "cyber_grid" | "cream_note" | "arena_score" | "sunset_gradient" | "ink_stamp"
        | "glass_card" | "tactical_blue" | "midnight_lux" | "candy_pop" | "forest_match"
        | "steel_gray" | "royal_gold" | "ocean_wave" | "lava_motion" | "pearl_lite"
        | "street_snap" | "comic_burst" | "vaporwave" | "newspaper" | "coffee_receipt"
        | "scoreboard_green" | "purple_stage" | "ice_blue" | "red_warning" | "kraft_label"
        | "mint_mono" | "black_gold" | "gradient_ring" | "billiard_felt" | "tournament_bracket"
        | "soft_shadow" | "bold_blocks" | "pink_soda" | "desert_sand" | "matrix_code"
        | "club_vip" | "clean_blue" | "orange_zine" | "silver_card" | "green_laser"
        | "classic_serif" => CopyLimits {
            title_chars: 30,
            body_chars: 112,
            body_lines: 7,
            tags_count: 3,
            tag_chars: 12,
        },
        _ => CopyLimits {
            title_chars: 30,
            body_chars: 96,
            body_lines: 6,
            tags_count: 3,
            tag_chars: 12,
        },
    }
}

pub async fn generate_copy(
    State(state): State<crate::AppState>,
    headers: HeaderMap,
    Json(input): Json<AiRequest>,
) -> ApiResult<CopyItem> {
    let user_id = crate::auth::authenticate(&state.db, &headers).await?;
    let _permit = state
        .ai_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("AI 任务繁忙，请稍后重试"))?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request(
            "提示词不能为空且不能超过 20000 个字符",
        ));
    }
    let started = Instant::now();
    let limits = limits_for_template(input.template.as_deref());
    let result = async {
        let text = call_provider(&state, &single_prompt(&input.prompt, limits), 520).await?;
        let item = parse_item(&text).ok_or_else(|| ApiError::internal("AI 返回内容无法解析"))?;
        fit_item_with_retry(&state, &input.prompt, item, limits).await
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
    let _permit = state
        .ai_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("AI 任务繁忙，请稍后重试"))?;
    if input.prompt.trim().is_empty() || input.prompt.chars().count() > 20000 {
        return Err(ApiError::bad_request(
            "提示词不能为空且不能超过 20000 个字符",
        ));
    }
    let count = input.count.clamp(1, 100);
    let started = Instant::now();
    let limits = limits_for_template(input.template.as_deref());
    let prompt = batch_prompt(&input.prompt, count, limits);
    let result = async {
        let text = call_provider(&state, &prompt, (count as u32 * 600).min(12000)).await?;
        let items = parse_batch(&text)
            .into_iter()
            .take(count)
            .map(|item| compact_item(item, limits))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return Err(ApiError::internal("AI 批量返回内容无法解析"));
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
    if state.config.ai_api_key.trim().is_empty() {
        return Err(ApiError::unavailable("服务器尚未配置 AI Provider"));
    }
    let endpoint = if state.config.ai_base_url.ends_with("/chat/completions") {
        state.config.ai_base_url.clone()
    } else {
        format!(
            "{}/chat/completions",
            state.config.ai_base_url.trim_end_matches('/')
        )
    };
    let response = state
        .ai_client
        .post(endpoint)
        .bearer_auth(&state.config.ai_api_key)
        .json(&ChatRequest {
            model: &state.config.ai_model,
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

fn single_prompt(prompt: &str, limits: CopyLimits) -> String {
    format!(
        "{prompt}\n\n{limits}\n严格按以下格式输出，不要添加解释：\n标题：xxx\n正文：xxx\n话题：#xx #xx",
        limits = limit_instruction(limits)
    )
}

fn batch_prompt(prompt: &str, count: usize, limits: CopyLimits) -> String {
    format!(
        "{prompt}\n\n{limits}\n生成{count}条差异化内容，每条按【第N条】分隔。每条严格包含：标题：xxx、正文：xxx、话题：#xx #xx",
        limits = limit_instruction(limits)
    )
}

fn limit_instruction(limits: CopyLimits) -> String {
    format!(
        "【模板容量限制：最高优先级】标题不超过{}字；正文不超过{}字且最多{}行；话题最多{}个，每个不超过{}字。宁可短一点，也不能超过模板承载范围。",
        limits.title_chars,
        limits.body_chars,
        limits.body_lines,
        limits.tags_count,
        limits.tag_chars
    )
}

async fn fit_item_with_retry(
    state: &crate::AppState,
    source_prompt: &str,
    item: CopyItem,
    limits: CopyLimits,
) -> Result<CopyItem, ApiError> {
    let item = normalize_item(item);
    if item_fits(&item, limits) {
        return Ok(item);
    }

    let prompt = compact_prompt(source_prompt, &item, limits);
    let compacted = match call_provider(state, &prompt, 420).await {
        Ok(text) => parse_item(&text).map(normalize_item),
        Err(error) => {
            tracing::warn!(error = %error.message, "AI compact retry failed; applying local copy limits");
            None
        }
    };
    Ok(compacted
        .filter(|candidate| item_fits(candidate, limits))
        .unwrap_or_else(|| compact_item(item, limits)))
}

fn compact_prompt(source_prompt: &str, item: &CopyItem, limits: CopyLimits) -> String {
    format!(
        "{source_prompt}\n\n下面这条文案超过模板承载范围，请保留原意并压缩到容量内。\n标题：{title}\n正文：{body}\n话题：{tags}\n\n{limits}\n只输出：\n标题：xxx\n正文：xxx\n话题：#xx #xx",
        title = item.title,
        body = item.body,
        tags = item.tags,
        limits = limit_instruction(limits)
    )
}

fn item_fits(item: &CopyItem, limits: CopyLimits) -> bool {
    visible_len(&item.title) <= limits.title_chars
        && visible_len(&item.body) <= limits.body_chars
        && item
            .body
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            <= limits.body_lines
        && item
            .tags
            .split_whitespace()
            .filter(|tag| !tag.trim().is_empty())
            .count()
            <= limits.tags_count
        && item
            .tags
            .split_whitespace()
            .all(|tag| visible_len(tag) <= limits.tag_chars)
}

fn compact_item(item: CopyItem, limits: CopyLimits) -> CopyItem {
    let item = normalize_item(item);
    CopyItem {
        id: item.id,
        title: truncate_visible(&item.title.replace('\n', " "), limits.title_chars),
        body: compact_body(&item.body, limits),
        tags: compact_tags(&item.tags, limits),
    }
}

fn normalize_item(mut item: CopyItem) -> CopyItem {
    item.title = collapse_inline_space(&item.title);
    item.body = item
        .body
        .lines()
        .map(collapse_inline_space)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    item.tags = item
        .tags
        .split_whitespace()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    item
}

fn compact_body(body: &str, limits: CopyLimits) -> String {
    let by_chars = truncate_visible(body, limits.body_chars);
    let mut lines = by_chars
        .lines()
        .map(collapse_inline_space)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() > limits.body_lines {
        lines.truncate(limits.body_lines);
        if let Some(last) = lines.last_mut() {
            if !last.ends_with('…') {
                last.push('…');
            }
        }
    }
    lines.join("\n")
}

fn compact_tags(tags: &str, limits: CopyLimits) -> String {
    tags.split_whitespace()
        .take(limits.tags_count)
        .map(|tag| truncate_visible(tag, limits.tag_chars))
        .collect::<Vec<_>>()
        .join(" ")
}

fn collapse_inline_space(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visible_len(value: &str) -> usize {
    value.chars().filter(|ch| !ch.is_whitespace()).count()
}

fn truncate_visible(value: &str, limit: usize) -> String {
    if visible_len(value) <= limit {
        return value.trim().to_string();
    }
    if limit == 0 {
        return String::new();
    }
    let mut output = String::new();
    let mut count = 0;
    for ch in value.trim().chars() {
        if !ch.is_whitespace() {
            if count + 1 >= limit {
                break;
            }
            count += 1;
        }
        output.push(ch);
    }
    output.trim_end().to_string() + "…"
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
    fn compacts_copy_to_template_limits() {
        let limits = CopyLimits {
            title_chars: 6,
            body_chars: 12,
            body_lines: 2,
            tags_count: 2,
            tag_chars: 4,
        };
        let item = CopyItem {
            id: None,
            title: "这个标题明显太长".into(),
            body: "第一行真的很长很长\n第二行也很长很长\n第三行不该保留".into(),
            tags: "#珠海台球 #超级长话题 #多余话题".into(),
        };
        let compacted = compact_item(item, limits);
        assert!(item_fits(&compacted, limits));
        assert!(compacted.title.ends_with('…'));
        assert_eq!(compacted.tags.split_whitespace().count(), 2);
    }
}
