use crate::models::{CopyFitLimits, RenderRequest};
use std::sync::{Arc, OnceLock};

pub const WIDTH: u32 = 1080;
pub const HEIGHT: u32 = 1920;
const FRAME_X: u32 = 60;
const FRAME_Y: u32 = 220;
const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 1480;
static FONT_DATABASE: OnceLock<Arc<fontdb::Database>> = OnceLock::new();

pub fn render_png(request: &RenderRequest) -> Result<Vec<u8>, String> {
    let svg = svg_for(request);
    let options = usvg::Options {
        fontdb: font_database(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options)
        .map_err(|error| format!("SVG parse failed: {error}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT)
        .ok_or_else(|| "unable to allocate render surface".to_string())?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|error| format!("PNG encode failed: {error}"))
}

fn font_database() -> Arc<fontdb::Database> {
    FONT_DATABASE
        .get_or_init(|| {
            let mut database = fontdb::Database::new();
            database.load_font_data(include_bytes!("../fonts/NotoSansCJKsc-Regular.otf").to_vec());
            database
                .load_font_data(include_bytes!("../fonts/NotoSerifCJKsc-SemiBold.otf").to_vec());
            Arc::new(database)
        })
        .clone()
}

pub fn svg_for(request: &RenderRequest) -> String {
    match request.template.as_str() {
        "fresh" => fresh(request),
        "minimal" => minimal(request),
        "poster" => poster(request),
        "journal" => journal(request),
        "magazine_pro" => magazine_pro(request),
        _ => magazine(request),
    }
}

pub fn copy_limits_for_template(template: &str) -> CopyFitLimits {
    match template {
        "minimal" => CopyFitLimits {
            title_chars: 30,
            body_chars: 136,
            body_lines: 8,
            tags_count: 3,
            tag_chars: 12,
        },
        "poster" => CopyFitLimits {
            title_chars: 30,
            body_chars: 144,
            body_lines: 8,
            tags_count: 3,
            tag_chars: 12,
        },
        "magazine_pro" | "fresh" | "journal" => CopyFitLimits {
            title_chars: 30,
            body_chars: 112,
            body_lines: 7,
            tags_count: 3,
            tag_chars: 12,
        },
        _ => CopyFitLimits {
            title_chars: 30,
            body_chars: 96,
            body_lines: 6,
            tags_count: 3,
            tag_chars: 12,
        },
    }
}

fn magazine(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 132;
    let content_right = 948;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 78),
        3,
        78,
        content_x,
        570,
        &colors.text,
        "Noto Serif CJK SC, Noto Serif SC, serif",
        700,
        1.25,
    );
    let divider_y = title_last_y + 64;
    let body_y = divider_y + 100;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        48,
        40,
        content_x,
        body_y,
        1370,
        &colors.body,
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.65,
    );
    let tags = tag_lines(
        &request.tags,
        content_x,
        content_right,
        1510,
        &colors.muted,
        28,
    );
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <defs><linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='{g1}'/><stop offset='1' stop-color='{g2}'/></linearGradient><filter id='shadow'><feGaussianBlur stdDeviation='24'/></filter></defs>
        <rect width='100%' height='100%' fill='#0E0E14'/><circle cx='930' cy='90' r='420' fill='{g1}' opacity='.28' filter='url(#shadow)'/><circle cx='90' cy='1790' r='360' fill='{g2}' opacity='.24' filter='url(#shadow)'/>
        <rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}' rx='40' fill='#FFFFFF' opacity='.98'/><text x='{content_x}' y='390' fill='{accent}' font-size='64' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text><line x1='260' y1='366' x2='600' y2='366' stroke='{accent}' stroke-width='2' opacity='.7'/><text x='{content_right}' y='390' text-anchor='end' fill='{accent}' font-size='30' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>
        {title}<rect x='{content_x}' y='{divider_y}' width='120' height='6' rx='3' fill='{accent}'/>{body}<line x1='{content_x}' y1='1450' x2='{content_right}' y2='1450' stroke='#EEEEEE'/>{tags}
        </svg>",
        g1 = colors.g1, g2 = colors.g2, accent = colors.accent, num = xml(&request.num), tag = xml(&request.tag)
    )
}

fn magazine_pro(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 128;
    let content_right = 952;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 76),
        3,
        76,
        content_x,
        560,
        "#1A1A1A",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        700,
        1.24,
    );
    let body_y = title_last_y + 120;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        46,
        38,
        content_x,
        body_y,
        1370,
        "#333333",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(
        &request.tags,
        content_x,
        content_right,
        1510,
        &colors.accent,
        28,
    );
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <defs><radialGradient id='g1'><stop stop-color='{g1}' stop-opacity='.9'/><stop offset='1' stop-color='{g1}' stop-opacity='0'/></radialGradient><radialGradient id='g2'><stop stop-color='{g2}' stop-opacity='.85'/><stop offset='1' stop-color='{g2}' stop-opacity='0'/></radialGradient><linearGradient id='card' x1='0' y1='0' x2='1' y2='1'><stop stop-color='#FFFFFF'/><stop offset='1' stop-color='#F6F6FB'/></linearGradient></defs>
        <rect width='100%' height='100%' fill='#0A0A12'/><circle cx='940' cy='80' r='470' fill='url(#g1)'/><circle cx='90' cy='1780' r='430' fill='url(#g2)'/><circle cx='580' cy='980' r='260' fill='{accent}' opacity='.16'/><rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}' rx='36' fill='url(#card)'/><rect x='61' y='221' width='958' height='1478' rx='35' fill='none' stroke='{accent}' stroke-opacity='.35' stroke-width='2'/>
        <text x='{content_x}' y='390' fill='{accent}' font-size='72' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text><line x1='260' y1='368' x2='610' y2='368' stroke='{accent}' stroke-opacity='.55'/><text x='{content_right}' y='390' text-anchor='end' fill='{accent}' font-size='26' letter-spacing='5' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}{body}<line x1='{content_x}' y1='1450' x2='{content_right}' y2='1450' stroke='#ECECF0'/>{tags}
        </svg>",
        g1 = colors.g1, g2 = colors.g2, accent = colors.accent, num = xml(&request.num), tag = xml(&request.tag)
    )
}

fn fresh(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 124;
    let content_right = 956;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 72),
        3,
        72,
        content_x,
        540,
        "#1A1A1A",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        700,
        1.26,
    );
    let divider_y = title_last_y + 60;
    let body_y = divider_y + 100;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        48,
        40,
        content_x,
        body_y,
        1370,
        "#333333",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_pills(&request.tags, &colors.g1, content_x, content_right, 1480);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><defs><linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='{g1}'/><stop offset='1' stop-color='{g2}'/></linearGradient><clipPath id='fresh-card-clip'><rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}' rx='36'/></clipPath></defs><rect width='100%' height='100%' fill='url(#bg)'/><rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}' rx='36' fill='#FFFFFF'/><rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='10' fill='{accent}' clip-path='url(#fresh-card-clip)'/>{pill}<text x='152' y='376' fill='#FFFFFF' font-size='34' font-weight='600' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<rect x='{content_x}' y='{divider_y}' width='96' height='6' rx='3' fill='{accent}'/>{body}{tags}</svg>",
        g1 = colors.g1, g2 = colors.g2, accent = colors.accent, tag = xml(&request.tag), pill = "<rect x='124' y='330' width='260' height='68' rx='30' fill='#1A1A1A' opacity='.9'/>"
    )
}

fn minimal(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 90;
    let content_right = 990;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 84),
        3,
        84,
        content_x,
        500,
        "#111111",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        700,
        1.22,
    );
    let divider_y = title_last_y + 72;
    let body_y = divider_y + 105;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        50,
        42,
        content_x,
        body_y,
        1490,
        "#333333",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.66,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1610, "#999999", 30);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#FAFAF7'/><text x='{content_x}' y='330' fill='{accent}' font-size='28' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<line x1='{content_x}' y1='{divider_y}' x2='{content_right}' y2='{divider_y}' stroke='#DDDDDD'/>{body}{tags}</svg>",
        accent = colors.accent, tag = xml(&request.tag)
    )
}

fn poster(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 80;
    let content_right = 1000;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 90),
        3,
        90,
        content_x,
        500,
        "#FFFFFF",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.18,
    );
    let divider_y = title_last_y + 72;
    let body_y = divider_y + 105;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        48,
        40,
        content_x,
        body_y,
        1490,
        "#F1F1F1",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1630, "#D7E7F0", 30);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><defs><linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='{g1}'/><stop offset='1' stop-color='{g2}'/></linearGradient></defs><rect width='100%' height='100%' fill='url(#bg)'/><circle cx='980' cy='420' r='260' fill='#FFFFFF' opacity='.12'/><text x='{content_x}' y='330' fill='#FFFFFF' fill-opacity='.8' font-size='30' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<rect x='{content_x}' y='{divider_y}' width='100' height='5' rx='2' fill='#FFFFFF' opacity='.8'/>{body}{tags}</svg>",
        g1 = colors.g1, g2 = colors.g2, tag = xml(&request.tag)
    )
}

fn journal(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 128;
    let content_right = 952;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 74),
        3,
        74,
        content_x,
        540,
        "#3A2A10",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        700,
        1.26,
    );
    let body_y = title_last_y + 122;
    let body = body_lines(
        &request.body,
        content_x,
        content_right,
        48,
        40,
        content_x,
        body_y,
        1380,
        "#4A3820",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1520, "#9A7A40", 30);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#F5EFE0'/><path d='M0 500H1080M0 580H1080M0 660H1080M0 740H1080M0 820H1080M0 900H1080M0 980H1080M0 1060H1080M0 1140H1080M0 1220H1080M0 1300H1080M0 1380H1080' stroke='#8A6A3A' stroke-opacity='.08'/><rect x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}' rx='28' fill='#FFFBF0'/><rect x='61' y='221' width='958' height='1478' rx='27' fill='none' stroke='{accent}' stroke-opacity='.2' stroke-width='2'/><rect x='460' y='200' width='160' height='44' rx='4' fill='{accent}' opacity='.35'/><text x='{content_x}' y='390' fill='{accent}' font-size='32' font-weight='600' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><line x1='{content_x}' y1='415' x2='360' y2='415' stroke='{accent}' stroke-width='3'/>{title}{body}<line x1='{content_x}' y1='1450' x2='{content_right}' y2='1450' stroke='{accent}' stroke-opacity='.18'/>{tags}</svg>",
        accent = colors.accent, tag = xml(&request.tag)
    )
}

struct Colors {
    g1: String,
    g2: String,
    accent: String,
    text: String,
    body: String,
    muted: String,
}

fn colors(request: &RenderRequest) -> Colors {
    Colors {
        g1: color(&request.glow1, "#FF8A5C"),
        g2: color(&request.glow2, "#FF5E62"),
        accent: color(&request.accent, "#FF5E62"),
        text: "#1A1A1A".into(),
        body: "#333333".into(),
        muted: "#999999".into(),
    }
}

fn color(value: &str, fallback: &str) -> String {
    let valid = (value.len() == 4 || value.len() == 7)
        && value.starts_with('#')
        && value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit());
    if valid {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn text_lines(
    text: &str,
    max_chars: usize,
    max_lines: usize,
    size: u32,
    x: u32,
    y: u32,
    fill: &str,
    family: &str,
    weight: u32,
    line_height: f32,
) -> (String, u32) {
    let lines = limited_lines(text, max_chars, max_lines);
    let line_step = (size as f32 * line_height) as u32;
    let last_baseline = y + (lines.len().saturating_sub(1) as u32 * line_step);
    let svg = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let baseline = y + (index as u32 * line_step);
            format!("<text x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-weight='{weight}' font-family='{family}'>{}</text>", xml(line))
        })
        .collect::<Vec<_>>()
        .join("");
    (svg, last_baseline)
}

#[allow(clippy::too_many_arguments)]
fn body_lines(
    text: &str,
    left: u32,
    right: u32,
    size: u32,
    min_size: u32,
    x: u32,
    y: u32,
    max_baseline: u32,
    fill: &str,
    family: &str,
    weight: u32,
    line_height: f32,
) -> String {
    let layout = fitting_body_layout(
        text,
        left,
        right,
        size,
        min_size,
        y,
        max_baseline,
        line_height,
    );
    let line_step = (layout.size as f32 * layout.line_height) as u32;
    let lines = if layout.truncated {
        limited_lines(text, layout.max_chars, layout.max_lines)
    } else {
        wrap_text(text, layout.max_chars)
    };
    lines.iter().enumerate().map(|(index, line)| {
        let baseline = y + (index as u32 * line_step);
        let size = layout.size;
        format!("<text x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-weight='{weight}' font-family='{family}'>{}</text>", xml(line))
    }).collect::<Vec<_>>().join("")
}

#[derive(Debug)]
struct BodyLayout {
    size: u32,
    line_height: f32,
    max_chars: usize,
    max_lines: usize,
    truncated: bool,
}

#[allow(clippy::too_many_arguments)]
fn fitting_body_layout(
    text: &str,
    left: u32,
    right: u32,
    base_size: u32,
    min_size: u32,
    y: u32,
    max_baseline: u32,
    base_line_height: f32,
) -> BodyLayout {
    let min_size = min_size.min(base_size).max(1);
    let mut fallback = layout_for_body(
        left,
        right,
        min_size,
        y,
        max_baseline,
        base_line_height,
        true,
    );
    let mut size = base_size;
    loop {
        for line_height in [
            base_line_height,
            (base_line_height - 0.08).max(1.34),
            (base_line_height - 0.14).max(1.28),
        ] {
            let layout = layout_for_body(left, right, size, y, max_baseline, line_height, false);
            if wrap_text(text, layout.max_chars).len() <= layout.max_lines {
                return layout;
            }
            if size == min_size {
                fallback = layout_for_body(left, right, size, y, max_baseline, line_height, true);
            }
        }
        if size == min_size {
            return fallback;
        }
        size = size.saturating_sub(2).max(min_size);
    }
}

fn layout_for_body(
    left: u32,
    right: u32,
    size: u32,
    y: u32,
    max_baseline: u32,
    line_height: f32,
    truncated: bool,
) -> BodyLayout {
    let line_step = (size as f32 * line_height) as u32;
    let max_lines = (max_baseline.saturating_sub(y) / line_step + 1).max(1) as usize;
    BodyLayout {
        size,
        line_height,
        max_chars: chars_for_width(left, right, size),
        max_lines,
        truncated,
    }
}

fn chars_for_width(left: u32, right: u32, font_size: u32) -> usize {
    (right.saturating_sub(left) / font_size).max(1) as usize
}

fn tag_lines(tags: &str, x: u32, right: u32, y: u32, fill: &str, size: u32) -> String {
    let line_step = (size as f32 * 1.4) as u32;
    limited_word_lines(tags, chars_for_width(x, right, size), 2)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let baseline = y + (index as u32 * line_step);
            format!("<text x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{}</text>", xml(line))
        })
        .collect::<Vec<_>>()
        .join("")
}

fn tag_pills(tags: &str, fill: &str, x: u32, right: u32, y: u32) -> String {
    let parts: Vec<&str> = tags.split_whitespace().collect();
    let mut result = String::new();
    let mut cursor = x;
    let mut row_y = y;
    let max_chars = ((right.saturating_sub(x).saturating_sub(40)) / 26).max(2) as usize;
    for part in parts.iter().take(5) {
        let label = if part.chars().count() > max_chars {
            format!(
                "{}…",
                part.chars()
                    .take(max_chars.saturating_sub(1))
                    .collect::<String>()
            )
        } else {
            (*part).to_string()
        };
        let width = 40 + (label.chars().count() as u32 * 26);
        if cursor > x && cursor + width > right {
            cursor = x;
            row_y += 66;
        }
        result.push_str(&format!("<rect x='{cursor}' y='{row_y}' width='{width}' height='52' rx='20' fill='{fill}'/><text x='{}' y='{}' fill='#FFFFFF' font-size='26' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{}</text>", cursor + 20, row_y + 35, xml(&label)));
        cursor += width + 14;
    }
    result
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for ch in raw.chars() {
            current.push(ch);
            if current.chars().count() >= max_chars {
                lines.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

fn limited_lines(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let mut lines = wrap_text(text, max_chars);
    if lines.len() <= max_lines {
        return lines;
    }
    lines.truncate(max_lines);
    if let Some(last) = lines.last_mut() {
        while last.chars().count() >= max_chars {
            last.pop();
        }
        last.push('…');
    }
    lines
}

fn limited_word_lines(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let word = if word.chars().count() > max_chars {
            format!(
                "{}…",
                word.chars()
                    .take(max_chars.saturating_sub(1))
                    .collect::<String>()
            )
        } else {
            word.to_string()
        };
        let candidate_len =
            current.chars().count() + usize::from(!current.is_empty()) + word.chars().count();
        if !current.is_empty() && candidate_len > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() <= max_lines {
        return lines;
    }
    lines.truncate(max_lines);
    if let Some(last) = lines.last_mut() {
        while last.chars().count() >= max_chars {
            last.pop();
        }
        last.push('…');
    }
    lines
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
    }

    #[test]
    fn wraps_chinese_text_deterministically() {
        assert_eq!(wrap_text("一二三四五六", 3), vec!["一二三", "四五六"]);
    }

    #[test]
    fn escapes_svg_text() {
        assert_eq!(xml("a<&"), "a&lt;&amp;");
    }

    #[test]
    fn truncates_long_text_inside_canvas() {
        let lines = limited_lines(&"很长的正文".repeat(100), 10, 4);
        assert_eq!(lines.len(), 4);
        assert!(lines[3].ends_with('…'));
        assert!(lines[3].chars().count() <= 10);
    }

    #[test]
    fn all_templates_emit_canvas() {
        let request = RenderRequest {
            title: "测试".into(),
            body: "正文".into(),
            ..Default::default()
        };
        for template in [
            "magazine",
            "magazine_pro",
            "fresh",
            "minimal",
            "poster",
            "journal",
        ] {
            let mut request = request.clone();
            request.template = template.into();
            let svg = svg_for(&request);
            assert!(svg.contains("width='1080'") && svg.contains("height='1920'"));
        }
    }

    #[test]
    fn framed_templates_share_the_same_outer_bounds() {
        let request = RenderRequest {
            title: "边框对齐测试".into(),
            body: "正文".into(),
            ..Default::default()
        };
        let expected_frame =
            format!("x='{FRAME_X}' y='{FRAME_Y}' width='{FRAME_WIDTH}' height='{FRAME_HEIGHT}'");
        for template in ["magazine", "magazine_pro", "fresh", "journal"] {
            let mut request = request.clone();
            request.template = template.into();
            assert!(
                svg_for(&request).contains(&expected_frame),
                "{template} frame does not use the shared bounds"
            );
        }
    }

    #[test]
    fn text_and_pills_respect_horizontal_layout() {
        let (text, _) = text_lines(
            "一二三四五六七八",
            chars_for_width(100, 500, 50),
            2,
            50,
            100,
            200,
            "#000000",
            "sans-serif",
            400,
            1.5,
        );
        assert!(text.contains("x='100'"));
        assert_eq!(wrap_text("一二三四五六七八", 8), vec!["一二三四五六七八"]);

        let pills = tag_pills("#一二三四 #五六七八 #九十十一", "#000000", 100, 500, 200);
        assert!(!pills.contains("x='500'"));
        assert!(pills.contains("y='266'"), "overflowing pills should wrap");
        assert_eq!(
            limited_word_lines("#珠海台球 #约球日常 #新手入门", 10, 2),
            vec!["#珠海台球", "#约球日常…"]
        );
    }

    #[test]
    fn template_svg_snapshots_are_stable() {
        let request = RenderRequest {
            num: "08".into(),
            tag: "SNAPSHOT".into(),
            title: "周末约球实战记录".into(),
            body: "第一局找手感\n第二局练走位\n最后来一盘抢五".into(),
            tags: "#珠海台球 #约球日常".into(),
            ..Default::default()
        };
        let expected = [
            4_370_394_096_670_118_294,
            2_915_345_514_686_357_144,
            16_951_864_457_228_097_559,
            12_331_733_539_329_250_638,
            1_902_716_855_967_025_275,
            12_621_961_104_212_921_327,
        ];
        let actual = [
            "magazine",
            "magazine_pro",
            "fresh",
            "minimal",
            "poster",
            "journal",
        ]
        .iter()
        .map(|template| {
            let mut request = request.clone();
            request.template = (*template).into();
            fnv1a(svg_for(&request).as_bytes())
        })
        .collect::<Vec<_>>();
        assert_eq!(actual, expected, "template snapshot changed");
    }

    #[test]
    fn renders_png_bytes() {
        let request = RenderRequest {
            title: "测试标题".into(),
            body: "第一行\n第二行".into(),
            ..Default::default()
        };
        let png = render_png(&request).expect("render should succeed");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.len() > 10_000);
    }

    #[test]
    fn renders_special_characters() {
        let request = RenderRequest {
            title: "球房 <A&B> '测试'".into(),
            body: "比分 8 > 7，价格 \"合理\" & 稳定".into(),
            tags: "#台球 #A&B".into(),
            ..Default::default()
        };
        assert!(render_png(&request).is_ok());
    }

    #[test]
    #[ignore = "manual template alignment visual review"]
    fn writes_template_alignment_samples() {
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/template-alignment-review");
        fs::create_dir_all(&directory).expect("create visual review directory");
        for template in [
            "magazine",
            "magazine_pro",
            "fresh",
            "minimal",
            "poster",
            "journal",
        ] {
            let request = RenderRequest {
                template: template.into(),
                num: "08".into(),
                tag: "BILLIARDS".into(),
                title: "周末约球实战记录：新手也能稳稳上手".into(),
                body: "第一局先找手感，不急着发力\n第二局专门练走位，把每一杆想清楚\n最后来一盘抢五，输赢都开心".into(),
                tags: "#珠海台球 #约球日常 #新手入门 #周末约球 #台球搭子".into(),
                ..Default::default()
            };
            fs::write(
                directory.join(format!("{template}.png")),
                render_png(&request).expect("render alignment sample"),
            )
            .expect("write alignment sample");
        }
    }

    #[test]
    #[ignore = "manual 100-image stability test"]
    fn renders_hundred_images_without_external_dependencies() {
        for index in 0..100 {
            let request = RenderRequest {
                template: [
                    "magazine",
                    "magazine_pro",
                    "fresh",
                    "minimal",
                    "poster",
                    "journal",
                ][index % 6]
                    .into(),
                num: format!("{:02}", index + 1),
                title: format!("第{}张离线稳定性测试", index + 1),
                body: "内置字体与 Rust 原生渲染，不依赖 Python、Chrome 或网络。".into(),
                tags: "#珠海台球 #批量测试".into(),
                ..Default::default()
            };
            let png = render_png(&request).expect("batch image should render");
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        }
    }
}
