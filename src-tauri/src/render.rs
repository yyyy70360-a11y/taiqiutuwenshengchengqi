use crate::models::RenderRequest;
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

const GENERATED_TEMPLATE_IDS: &[&str] = &[
    "aurora",
    "espresso",
    "sunset",
    "ocean",
    "rainforest",
    "chalkboard",
    "luxury",
    "arcade",
    "comic",
    "paper_cut",
    "gradient_card",
    "ink_brush",
    "tech_hud",
    "warm_note",
    "ice",
    "candy",
    "carbon",
    "ceramic",
    "matchday",
    "lounge",
    "diagonal",
    "retro_tv",
    "terrazzo",
    "silk",
    "desert",
    "mint",
    "royal_blue",
    "darkroom",
    "data_panel",
    "sticker_bomb",
    "prism",
    "stadium",
    "typewriter",
    "zen",
];

fn generated_template_seed(template: &str) -> Option<usize> {
    GENERATED_TEMPLATE_IDS
        .iter()
        .position(|candidate| *candidate == template)
}

pub fn svg_for(request: &RenderRequest) -> String {
    let svg = if let Some(seed) = generated_template_seed(request.template.as_str()) {
        designed_template(request, seed)
    } else {
        match request.template.as_str() {
            "fresh" => fresh(request),
            "minimal" => minimal(request),
            "poster" => poster(request),
            "journal" => journal(request),
            "neon" => neon(request),
            "newspaper" => newspaper(request),
            "blueprint" => blueprint(request),
            "polaroid" => polaroid(request),
            "scoreboard" => scoreboard(request),
            "vaporwave" => vaporwave(request),
            "classic" => classic(request),
            "mono" => mono(request),
            "club" => club(request),
            "street" => street(request),
            "magazine_pro" => magazine_pro(request),
            _ => magazine(request),
        }
    };
    apply_typography(svg, request)
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
        chars_for_width(content_x, content_right, 48),
        48,
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
        chars_for_width(content_x, content_right, 46),
        46,
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
        chars_for_width(content_x, content_right, 48),
        48,
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
        chars_for_width(content_x, content_right, 50),
        50,
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
        chars_for_width(content_x, content_right, 48),
        48,
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
        chars_for_width(content_x, content_right, 48),
        48,
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

fn neon(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 108;
    let content_right = 972;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 76),
        3,
        76,
        content_x,
        520,
        "#F7FBFF",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.2,
    );
    let body_y = title_last_y + 118;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1400,
        "#D7F7FF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1535, "#FFEB7A", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <defs><linearGradient id='neon-bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='#090A1A'/><stop offset='.52' stop-color='#171139'/><stop offset='1' stop-color='#061C2C'/></linearGradient><filter id='neon-glow'><feGaussianBlur stdDeviation='10'/></filter></defs>
        <rect width='100%' height='100%' fill='url(#neon-bg)'/>
        <path d='M0 320H1080M0 520H1080M0 720H1080M0 920H1080M0 1120H1080M0 1320H1080M0 1520H1080' stroke='{g1}' stroke-opacity='.12'/><path d='M120 0V1920M320 0V1920M520 0V1920M720 0V1920M920 0V1920' stroke='{g2}' stroke-opacity='.10'/>
        <rect x='70' y='230' width='940' height='1450' rx='34' fill='#0C1026' stroke='{g1}' stroke-width='3'/><rect x='72' y='232' width='936' height='1446' rx='32' fill='none' stroke='{g2}' stroke-width='2' opacity='.7' filter='url(#neon-glow)'/>
        <text x='{content_x}' y='392' fill='{g2}' font-size='36' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='392' text-anchor='end' fill='{g1}' font-size='70' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>
        {title}<line x1='{content_x}' y='{line_y}' x2='{content_right}' y2='{line_y}' stroke='{g2}' stroke-width='2' opacity='.55'/>{body}{tags}
        </svg>",
        g1 = colors.g1,
        g2 = colors.g2,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 52
    )
}

fn newspaper(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 92;
    let content_right = 988;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 72),
        3,
        72,
        content_x,
        500,
        "#1B1B18",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.18,
    );
    let body_y = title_last_y + 104;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 42),
        42,
        content_x,
        body_y,
        1450,
        "#2D2A24",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        400,
        1.58,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1585, "#686050", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <rect width='100%' height='100%' fill='#E7DDC8'/><path d='M0 0H1080V1920H0Z' fill='#F5EEDC'/><path d='M80 210H1000V1710H80Z' fill='none' stroke='#1B1B18' stroke-width='3'/><path d='M90 220H990V1700H90Z' fill='none' stroke='#1B1B18' stroke-width='1' opacity='.45'/>
        <text x='92' y='345' fill='#1B1B18' font-size='30' font-weight='700' letter-spacing='5' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>BILLIARDS DAILY</text><text x='988' y='345' text-anchor='end' fill='{accent}' font-size='30' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{num}</text><line x1='92' y1='378' x2='988' y2='378' stroke='#1B1B18' stroke-width='4'/><line x1='92' y1='392' x2='988' y2='392' stroke='#1B1B18'/>
        <text x='{content_x}' y='438' fill='{accent}' font-size='24' letter-spacing='4' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<line x1='{content_x}' y='{line_y}' x2='{content_right}' y2='{line_y}' stroke='#1B1B18' stroke-width='2'/>{body}{tags}
        </svg>",
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 42
    )
}

fn blueprint(request: &RenderRequest) -> String {
    let content_x = 104;
    let content_right = 976;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 68),
        3,
        68,
        content_x,
        520,
        "#F3FEFF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        800,
        1.22,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 42),
        42,
        content_x,
        body_y,
        1410,
        "#CDEEFF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.6,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1545, "#97D5FF", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <defs><pattern id='grid' width='40' height='40' patternUnits='userSpaceOnUse'><path d='M40 0H0V40' fill='none' stroke='#7CCFFF' stroke-opacity='.18' stroke-width='1'/></pattern><pattern id='grid-big' width='200' height='200' patternUnits='userSpaceOnUse'><path d='M200 0H0V200' fill='none' stroke='#7CCFFF' stroke-opacity='.35' stroke-width='2'/></pattern></defs>
        <rect width='100%' height='100%' fill='#06304B'/><rect width='100%' height='100%' fill='url(#grid)'/><rect width='100%' height='100%' fill='url(#grid-big)'/>
        <rect x='72' y='230' width='936' height='1450' fill='none' stroke='#BDEEFF' stroke-width='3'/><circle cx='900' cy='392' r='60' fill='none' stroke='#BDEEFF' stroke-opacity='.45'/><path d='M104 390H720M104 420H580' stroke='#BDEEFF' stroke-opacity='.7'/>
        <text x='{content_x}' y='360' fill='#BDEEFF' font-size='30' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='390' text-anchor='end' fill='#FFFFFF' font-size='56' font-weight='800' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{num}</text>
        {title}<path d='M{content_x} {line_y}H{content_right}' stroke='#BDEEFF' stroke-dasharray='18 12'/>{body}{tags}</svg>",
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 48
    )
}

fn polaroid(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 132;
    let content_right = 948;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 70),
        3,
        70,
        content_x,
        580,
        "#222222",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        800,
        1.22,
    );
    let body_y = title_last_y + 106;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1390,
        "#3D3D3D",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.64,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1545, "#777777", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>
        <defs><linearGradient id='photo-bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='{g1}'/><stop offset='1' stop-color='{g2}'/></linearGradient><filter id='paper-shadow'><feDropShadow dx='0' dy='20' stdDeviation='20' flood-opacity='.28'/></filter></defs>
        <rect width='100%' height='100%' fill='#201C22'/><circle cx='120' cy='180' r='330' fill='{g1}' opacity='.22'/><circle cx='980' cy='1740' r='380' fill='{g2}' opacity='.22'/>
        <g filter='url(#paper-shadow)'><rect x='82' y='260' width='916' height='1380' rx='18' fill='#FFFDF8'/><rect x='122' y='310' width='836' height='260' rx='14' fill='url(#photo-bg)'/><circle cx='810' cy='400' r='70' fill='#FFFFFF' opacity='.35'/><path d='M122 570L340 410L520 530L682 440L958 570Z' fill='#FFFFFF' opacity='.24'/></g>
        <text x='{content_x}' y='438' fill='#FFFFFF' font-size='34' font-weight='700' letter-spacing='5' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='1540' text-anchor='end' fill='{accent}' font-size='60' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>
        {title}<rect x='{content_x}' y='{line_y}' width='120' height='5' rx='2' fill='{accent}'/>{body}{tags}</svg>",
        g1 = colors.g1,
        g2 = colors.g2,
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 46
    )
}

fn scoreboard(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 96;
    let content_right = 984;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 72),
        3,
        72,
        content_x,
        545,
        "#F7FFF2",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        900,
        1.2,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1415,
        "#DFFFE0",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1545, "#A8F5A1", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#07120A'/><rect x='62' y='220' width='956' height='1480' rx='30' fill='#0B2512' stroke='#71FF64' stroke-opacity='.45' stroke-width='3'/><rect x='92' y='300' width='896' height='150' rx='14' fill='#102F18' stroke='#71FF64' stroke-opacity='.35'/>
        <text x='118' y='394' fill='#71FF64' font-size='40' font-weight='900' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>SCORE</text><text x='920' y='408' text-anchor='end' fill='{accent}' font-size='86' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>
        <text x='{content_x}' y='492' fill='#71FF64' font-size='26' letter-spacing='5' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='#71FF64' stroke-opacity='.55'/>{body}{tags}</svg>",
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 52
    )
}

fn vaporwave(request: &RenderRequest) -> String {
    let content_x = 112;
    let content_right = 968;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 72),
        3,
        72,
        content_x,
        565,
        "#FFFFFF",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.18,
    );
    let body_y = title_last_y + 110;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1410,
        "#FFF2FF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_pills(&request.tags, "#FF4FD8", content_x, content_right, 1510);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><defs><linearGradient id='vap' x1='0' y1='0' x2='0' y2='1'><stop stop-color='#3A1C71'/><stop offset='.52' stop-color='#D76D77'/><stop offset='1' stop-color='#FFAF7B'/></linearGradient></defs><rect width='100%' height='100%' fill='url(#vap)'/><circle cx='540' cy='410' r='190' fill='#FFD45E'/><path d='M350 365H730M330 420H750M360 475H720M410 530H670' stroke='#3A1C71' stroke-width='18' opacity='.48'/><path d='M0 1520L260 1180L420 1390L590 1120L1080 1520V1920H0Z' fill='#25104E' opacity='.68'/><path d='M0 1520H1080M0 1600H1080M0 1680H1080M0 1760H1080' stroke='#FFFFFF' stroke-opacity='.22'/>
        <text x='{content_x}' y='392' fill='#FFFFFF' font-size='28' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='392' text-anchor='end' fill='#FFFFFF' font-size='56' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>{title}<rect x='{content_x}' y='{line_y}' width='140' height='6' rx='3' fill='#00F5FF'/>{body}{tags}</svg>",
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 44
    )
}

fn classic(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 120;
    let content_right = 960;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 76),
        3,
        76,
        content_x,
        535,
        "#FFEBC0",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.2,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1405,
        "#F8E2B4",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1540, "#D8B46A", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#301014'/><rect x='72' y='228' width='936' height='1464' rx='28' fill='#4A151C' stroke='#D8B46A' stroke-width='4'/><rect x='96' y='252' width='888' height='1416' rx='20' fill='none' stroke='#D8B46A' stroke-opacity='.45' stroke-width='2'/><path d='M120 392H420M660 392H960' stroke='#D8B46A' stroke-width='3'/><circle cx='540' cy='392' r='56' fill='none' stroke='{accent}' stroke-width='4'/>
        <text x='540' y='412' text-anchor='middle' fill='#FFEBC0' font-size='44' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text><text x='{content_x}' y='465' fill='#D8B46A' font-size='28' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text>{title}<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='#D8B46A' stroke-opacity='.55'/>{body}{tags}</svg>",
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 48
    )
}

fn mono(request: &RenderRequest) -> String {
    let content_x = 88;
    let content_right = 992;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 82),
        3,
        82,
        content_x,
        500,
        "#111111",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.18,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 46),
        46,
        content_x,
        body_y,
        1485,
        "#222222",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.6,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1605, "#555555", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#F8F8F5'/><rect x='54' y='214' width='972' height='1492' fill='none' stroke='#111111' stroke-width='4'/><rect x='78' y='238' width='924' height='1444' fill='none' stroke='#111111' stroke-width='1'/><text x='{content_x}' y='350' fill='#111111' font-size='26' letter-spacing='10' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='350' text-anchor='end' fill='#111111' font-size='38' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>{title}<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='#111111' stroke-width='3'/>{body}{tags}</svg>",
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 48
    )
}

fn club(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 118;
    let content_right = 962;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 74),
        3,
        74,
        content_x,
        545,
        "#F8F6E8",
        "Noto Serif CJK SC, Noto Serif SC, serif",
        900,
        1.2,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1410,
        "#D6E8CF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        400,
        1.62,
    );
    let tags = tag_lines(&request.tags, content_x, content_right, 1545, "#BFD9A6", 28);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><defs><radialGradient id='felt'><stop stop-color='#1B6F3A'/><stop offset='1' stop-color='#053B20'/></radialGradient></defs><rect width='100%' height='100%' fill='#06160D'/><rect x='62' y='220' width='956' height='1480' rx='42' fill='url(#felt)' stroke='#C9A457' stroke-width='8'/><circle cx='132' cy='292' r='28' fill='#050505' opacity='.7'/><circle cx='948' cy='292' r='28' fill='#050505' opacity='.7'/><circle cx='132' cy='1628' r='28' fill='#050505' opacity='.7'/><circle cx='948' cy='1628' r='28' fill='#050505' opacity='.7'/><path d='M118 424H962' stroke='#F8F6E8' stroke-opacity='.35'/>
        <text x='{content_x}' y='390' fill='#F8F6E8' font-size='28' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='395' text-anchor='end' fill='{accent}' font-size='66' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>{title}<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='#C9A457' stroke-width='3'/>{body}{tags}</svg>",
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 48
    )
}

fn street(request: &RenderRequest) -> String {
    let colors = colors(request);
    let content_x = 100;
    let content_right = 980;
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, 74),
        3,
        74,
        content_x,
        535,
        "#FFFFFF",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        900,
        1.18,
    );
    let body_y = title_last_y + 112;
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, 44),
        44,
        content_x,
        body_y,
        1400,
        "#F5F5F5",
        "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        500,
        1.62,
    );
    let tags = tag_pills(&request.tags, &colors.g2, content_x, content_right, 1510);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'><rect width='100%' height='100%' fill='#161616'/><rect x='0' y='0' width='1080' height='420' fill='{g1}' opacity='.85'/><rect x='0' y='1500' width='1080' height='420' fill='{g2}' opacity='.85'/><path d='M40 340L260 240L430 340L690 210L1040 360' fill='none' stroke='#FFFFFF' stroke-width='12' stroke-linecap='round' stroke-linejoin='round' opacity='.25'/><rect x='72' y='250' width='936' height='1410' rx='24' fill='#1F1F1F' stroke='#FFFFFF' stroke-opacity='.28' stroke-width='3'/><rect x='92' y='270' width='896' height='1370' rx='18' fill='none' stroke='{accent}' stroke-width='3' stroke-dasharray='18 12'/>
        <text x='{content_x}' y='390' fill='#FFFFFF' font-size='32' font-weight='900' letter-spacing='5' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='405' text-anchor='end' fill='#FFFFFF' font-size='72' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>{title}<rect x='{content_x}' y='{line_y}' width='180' height='8' rx='4' fill='{accent}'/>{body}{tags}</svg>",
        g1 = colors.g1,
        g2 = colors.g2,
        accent = colors.accent,
        num = xml(&request.num),
        tag = xml(&request.tag),
        line_y = title_last_y + 46
    )
}

#[derive(Clone, Copy)]
struct DesignerPalette {
    bg1: &'static str,
    bg2: &'static str,
    surface: &'static str,
    text: &'static str,
    body: &'static str,
    muted: &'static str,
    accent: &'static str,
    title_family: &'static str,
    body_family: &'static str,
}

const GENERATED_PALETTES: &[DesignerPalette] = &[
    DesignerPalette {
        bg1: "#071629",
        bg2: "#2945A8",
        surface: "#101E3A",
        text: "#F8FEFF",
        body: "#DBF8FF",
        muted: "#9ADFFF",
        accent: "#76FFD8",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#2A1710",
        bg2: "#8B5E34",
        surface: "#FFF2DD",
        text: "#2C1B12",
        body: "#4C3425",
        muted: "#9A6B45",
        accent: "#C98D4E",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#FF7A59",
        bg2: "#5A2A62",
        surface: "#FFF4E8",
        text: "#301B2F",
        body: "#59314C",
        muted: "#95616D",
        accent: "#FF4E7A",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#06283D",
        bg2: "#47B5FF",
        surface: "#EAF8FF",
        text: "#06283D",
        body: "#17455E",
        muted: "#5E91A8",
        accent: "#00A8E8",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#083B25",
        bg2: "#7FB069",
        surface: "#F1F8E9",
        text: "#14351F",
        body: "#29492F",
        muted: "#6B8A61",
        accent: "#E7B10A",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#111C17",
        bg2: "#2E7D50",
        surface: "#16251D",
        text: "#F3FFE8",
        body: "#DCEFCF",
        muted: "#A5C59A",
        accent: "#E5C07B",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#080808",
        bg2: "#4A3212",
        surface: "#121212",
        text: "#F8E8B6",
        body: "#E7D39A",
        muted: "#AA8848",
        accent: "#D6AF52",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#17112E",
        bg2: "#FF006E",
        surface: "#111026",
        text: "#FFFFFF",
        body: "#D8D2FF",
        muted: "#99F7FF",
        accent: "#FFBE0B",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#FCE761",
        bg2: "#FF595E",
        surface: "#FFF9D8",
        text: "#1D1D1B",
        body: "#333333",
        muted: "#6F4A00",
        accent: "#1982C4",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#F6E7D8",
        bg2: "#E36F5A",
        surface: "#FFFDF7",
        text: "#3A2E2A",
        body: "#5E4840",
        muted: "#A5796B",
        accent: "#2F9C95",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#4E54C8",
        bg2: "#8F94FB",
        surface: "#FFFFFF",
        text: "#171A3A",
        body: "#36395C",
        muted: "#7C80BA",
        accent: "#FF7A59",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#F6F3EA",
        bg2: "#DAD3C1",
        surface: "#FFFDF6",
        text: "#181815",
        body: "#33322C",
        muted: "#8B826D",
        accent: "#111111",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#031B2D",
        bg2: "#005F73",
        surface: "#062437",
        text: "#EAFBFF",
        body: "#BDEAF5",
        muted: "#75C7D8",
        accent: "#94D2BD",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#F4B860",
        bg2: "#FFF6D6",
        surface: "#FFF9E8",
        text: "#3B2B1B",
        body: "#604322",
        muted: "#9C7B45",
        accent: "#E76F51",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#DFF7FF",
        bg2: "#7CC7E8",
        surface: "#F8FDFF",
        text: "#073B4C",
        body: "#21596B",
        muted: "#6A9BAD",
        accent: "#118AB2",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#FFB3C1",
        bg2: "#FFD6A5",
        surface: "#FFF5F7",
        text: "#4A2030",
        body: "#704355",
        muted: "#B36A82",
        accent: "#FF477E",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#0B0D10",
        bg2: "#2B3038",
        surface: "#15191F",
        text: "#F5F7FA",
        body: "#C8D0D8",
        muted: "#8A94A0",
        accent: "#E63946",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#F8F4EA",
        bg2: "#D8C3A5",
        surface: "#FFFCF5",
        text: "#2F2A22",
        body: "#544B3D",
        muted: "#9A8D78",
        accent: "#7A5C3A",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#06101F",
        bg2: "#D7263D",
        surface: "#0C182A",
        text: "#FFFFFF",
        body: "#DEE8F8",
        muted: "#98A8BF",
        accent: "#FBB13C",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#32213A",
        bg2: "#667761",
        surface: "#F4EFE6",
        text: "#2D2432",
        body: "#4F4157",
        muted: "#89788E",
        accent: "#A26769",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#0B132B",
        bg2: "#FF5A5F",
        surface: "#F7F7FF",
        text: "#10162F",
        body: "#333D5B",
        muted: "#7A86A5",
        accent: "#FF5A5F",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#2D132C",
        bg2: "#801336",
        surface: "#190B1A",
        text: "#FFEDE8",
        body: "#F2C7BD",
        muted: "#C58E84",
        accent: "#EE4540",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#EFE7DA",
        bg2: "#C2B8A3",
        surface: "#FAF7F0",
        text: "#2B2B2B",
        body: "#515151",
        muted: "#8D8372",
        accent: "#6A994E",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#3A0CA3",
        bg2: "#F72585",
        surface: "#F7F1FF",
        text: "#261447",
        body: "#513B72",
        muted: "#8E77AD",
        accent: "#4CC9F0",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#C97B45",
        bg2: "#F2D8A7",
        surface: "#FFF6E4",
        text: "#3A2416",
        body: "#5E3E27",
        muted: "#A87547",
        accent: "#9C6644",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#B7E4C7",
        bg2: "#52B788",
        surface: "#F3FFF8",
        text: "#123524",
        body: "#2D533E",
        muted: "#68A681",
        accent: "#40916C",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#061A40",
        bg2: "#0353A4",
        surface: "#071D49",
        text: "#F4F8FF",
        body: "#C9DAF8",
        muted: "#88A7D6",
        accent: "#E6B800",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#0C0B0B",
        bg2: "#3D2C2E",
        surface: "#161313",
        text: "#F6F1E8",
        body: "#D8CBBE",
        muted: "#9C8A7E",
        accent: "#C77D3A",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#02111B",
        bg2: "#064789",
        surface: "#061A2B",
        text: "#E7F6FF",
        body: "#B7DAEE",
        muted: "#6EA4C4",
        accent: "#F4D35E",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#FFCFD2",
        bg2: "#B8E0D2",
        surface: "#FFFDFB",
        text: "#2C2C34",
        body: "#55515E",
        muted: "#948FA0",
        accent: "#FF6B6B",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#1B1F3B",
        bg2: "#533A7B",
        surface: "#F9F7FF",
        text: "#17172A",
        body: "#3C3A54",
        muted: "#817A9E",
        accent: "#00B4D8",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#03120E",
        bg2: "#14532D",
        surface: "#081C15",
        text: "#F0FFF4",
        body: "#CEEED8",
        muted: "#8BC7A0",
        accent: "#B7E4C7",
        title_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
    DesignerPalette {
        bg1: "#F2E8CF",
        bg2: "#BC6C25",
        surface: "#FFFDF6",
        text: "#2B2118",
        body: "#4D3A2C",
        muted: "#9C7A58",
        accent: "#5F3DC4",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Serif CJK SC, Noto Serif SC, serif",
    },
    DesignerPalette {
        bg1: "#F7F4EA",
        bg2: "#D7E3C8",
        surface: "#FCFBF3",
        text: "#22251F",
        body: "#4A5141",
        muted: "#84906E",
        accent: "#6C8A4D",
        title_family: "Noto Serif CJK SC, Noto Serif SC, serif",
        body_family: "Noto Sans CJK SC, Noto Sans SC, sans-serif",
    },
];

fn designed_template(request: &RenderRequest, seed: usize) -> String {
    let palette = GENERATED_PALETTES[seed % GENERATED_PALETTES.len()];
    let g1 = palette.bg1;
    let g2 = palette.bg2;
    let accent = palette.accent;
    let content_x = [92, 108, 124, 140, 82, 118][seed % 6];
    let content_right = 1080 - content_x;
    let title_size = [68, 72, 76, 80, 74, 70][seed % 6];
    let body_size = [42, 44, 46, 48, 43, 45][seed % 6];
    let title_y = [500, 530, 560, 590, 515, 545][seed % 6];
    let body_max_y = [1390, 1420, 1450, 1480, 1410, 1465][seed % 6];
    let title_weight = if seed % 3 == 0 { 900 } else { 800 };
    let (title, title_last_y) = text_lines(
        &request.title,
        chars_for_width(content_x, content_right, title_size),
        3,
        title_size,
        content_x,
        title_y,
        palette.text,
        palette.title_family,
        title_weight,
        1.2,
    );
    let line_y = title_last_y + [42, 48, 54, 60][seed % 4];
    let body_y = line_y + [76, 88, 96, 108][seed % 4];
    let body = body_lines(
        &request.body,
        chars_for_width(content_x, content_right, body_size),
        body_size,
        content_x,
        body_y,
        body_max_y,
        palette.body,
        palette.body_family,
        if seed % 5 == 0 { 500 } else { 400 },
        [1.54, 1.58, 1.62, 1.66][seed % 4],
    );
    let tag_y = body_max_y + [112, 126, 140, 154][seed % 4];
    let tags = if seed % 4 == 0 {
        tag_pills(&request.tags, accent, content_x, content_right, tag_y)
    } else {
        tag_lines(
            &request.tags,
            content_x,
            content_right,
            tag_y,
            palette.muted,
            28,
        )
    };
    let background = designer_background(seed, g1, g2, accent);
    let card = designer_card(seed, palette, accent);
    let header = designer_header(seed, request, content_x, content_right, palette, accent);
    let divider = designer_divider(seed, content_x, content_right, line_y, palette, accent);
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{WIDTH}' height='{HEIGHT}' viewBox='0 0 {WIDTH} {HEIGHT}'>{background}{card}{header}{title}{divider}{body}{tags}</svg>"
    )
}

fn designer_background(seed: usize, g1: &str, g2: &str, accent: &str) -> String {
    let defs = format!(
        "<defs><linearGradient id='designer-bg' x1='0' y1='0' x2='1' y2='1'><stop stop-color='{g1}'/><stop offset='1' stop-color='{g2}'/></linearGradient><pattern id='designer-dot' width='54' height='54' patternUnits='userSpaceOnUse'><circle cx='8' cy='8' r='3' fill='{accent}' opacity='.28'/></pattern><pattern id='designer-grid' width='72' height='72' patternUnits='userSpaceOnUse'><path d='M72 0H0V72' fill='none' stroke='{accent}' stroke-opacity='.16'/></pattern><filter id='designer-blur'><feGaussianBlur stdDeviation='28'/></filter></defs>"
    );
    let motif = match seed % 10 {
        0 => format!("<circle cx='160' cy='220' r='310' fill='{accent}' opacity='.28' filter='url(#designer-blur)'/><circle cx='940' cy='1660' r='360' fill='{g2}' opacity='.28' filter='url(#designer-blur)'/>"),
        1 => format!("<rect x='0' y='0' width='1080' height='1920' fill='url(#designer-dot)'/><circle cx='880' cy='300' r='170' fill='none' stroke='{accent}' stroke-opacity='.32' stroke-width='24'/>"),
        2 => format!("<path d='M-80 500L1160 180L1160 430L-80 760Z' fill='{accent}' opacity='.24'/><path d='M-40 1540L1120 1220L1120 1460L-40 1780Z' fill='{g1}' opacity='.25'/>"),
        3 => format!("<rect width='1080' height='1920' fill='url(#designer-grid)'/><path d='M70 420H1010M70 760H1010M70 1100H1010M70 1440H1010' stroke='{accent}' stroke-opacity='.12'/>"),
        4 => format!("<path d='M0 0H1080V380Q780 530 540 390T0 470Z' fill='{accent}' opacity='.22'/><path d='M0 1600Q320 1470 540 1635T1080 1520V1920H0Z' fill='{g2}' opacity='.26'/>"),
        5 => format!("<path d='M120 220C360 100 560 360 800 210S1120 270 1160 180' fill='none' stroke='{accent}' stroke-opacity='.34' stroke-width='18'/><path d='M-60 1640C240 1450 520 1700 760 1530S1080 1540 1160 1430' fill='none' stroke='{g1}' stroke-opacity='.28' stroke-width='20'/>"),
        6 => format!("<rect x='0' y='0' width='1080' height='520' fill='{accent}' opacity='.34'/><rect x='0' y='1400' width='1080' height='520' fill='{g2}' opacity='.34'/>"),
        7 => format!("<circle cx='540' cy='360' r='190' fill='{accent}' opacity='.30'/><path d='M300 330H780M280 395H800M330 460H750' stroke='{g1}' stroke-width='16' opacity='.36'/>"),
        8 => format!("<path d='M0 280L1080 100V0H0Z' fill='{accent}' opacity='.30'/><path d='M0 1920H1080V1540L0 1740Z' fill='{g2}' opacity='.30'/><rect width='1080' height='1920' fill='url(#designer-dot)' opacity='.35'/>"),
        _ => format!("<rect width='1080' height='1920' fill='url(#designer-grid)' opacity='.65'/><circle cx='120' cy='1560' r='210' fill='{accent}' opacity='.22'/><circle cx='980' cy='340' r='250' fill='{g1}' opacity='.18'/>"),
    };
    format!("{defs}<rect width='100%' height='100%' fill='url(#designer-bg)'/>{motif}")
}

fn designer_card(seed: usize, palette: DesignerPalette, accent: &str) -> String {
    match seed % 8 {
        0 => format!("<rect x='66' y='230' width='948' height='1460' rx='42' fill='{surface}' opacity='.94'/><rect x='86' y='250' width='908' height='1420' rx='34' fill='none' stroke='{accent}' stroke-opacity='.36' stroke-width='3'/>", surface = palette.surface),
        1 => format!("<rect x='88' y='250' width='904' height='1398' rx='20' fill='{surface}' opacity='.96'/><rect x='116' y='278' width='848' height='1342' rx='12' fill='none' stroke='{accent}' stroke-opacity='.22'/>", surface = palette.surface),
        2 => format!("<rect x='56' y='216' width='968' height='1500' rx='0' fill='{surface}' opacity='.93'/><line x1='82' y1='242' x2='998' y2='242' stroke='{accent}' stroke-width='4'/><line x1='82' y1='1690' x2='998' y2='1690' stroke='{accent}' stroke-width='4'/>", surface = palette.surface),
        3 => format!("<rect x='102' y='276' width='876' height='1330' rx='34' fill='{surface}' opacity='.95'/><rect x='74' y='248' width='876' height='1330' rx='34' fill='none' stroke='{accent}' stroke-width='3' stroke-opacity='.28'/>", surface = palette.surface),
        4 => format!("<rect x='0' y='0' width='1080' height='1920' fill='{surface}' opacity='.90'/><rect x='70' y='230' width='940' height='1460' rx='26' fill='none' stroke='{accent}' stroke-opacity='.30' stroke-width='3'/>", surface = palette.surface),
        5 => format!("<g transform='rotate(-2 540 960)'><rect x='92' y='262' width='896' height='1370' rx='28' fill='{surface}' opacity='.94'/></g><rect x='118' y='288' width='844' height='1318' rx='18' fill='none' stroke='{accent}' stroke-opacity='.26'/>", surface = palette.surface),
        6 => format!("<rect x='60' y='220' width='960' height='1480' rx='44' fill='{surface}' opacity='.92'/><rect x='60' y='220' width='190' height='1480' rx='44' fill='{accent}' opacity='.16'/>", surface = palette.surface),
        _ => format!("<rect x='76' y='238' width='928' height='1444' rx='18' fill='{surface}' opacity='.95'/><path d='M76 470H1004M76 1450H1004' stroke='{accent}' stroke-opacity='.22' stroke-width='2'/>", surface = palette.surface),
    }
}

fn designer_header(
    seed: usize,
    request: &RenderRequest,
    content_x: u32,
    content_right: u32,
    palette: DesignerPalette,
    accent: &str,
) -> String {
    let num = xml(&request.num);
    let tag = xml(&request.tag);
    match seed % 6 {
        0 => format!("<text x='{content_x}' y='370' fill='{accent}' font-size='30' font-weight='700' letter-spacing='7' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='382' text-anchor='end' fill='{text}' font-size='66' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", text = palette.text),
        1 => format!("<rect x='{content_x}' y='326' width='260' height='62' rx='31' fill='{accent}' opacity='.92'/><text x='{label_x}' y='367' fill='{surface}' font-size='26' font-weight='700' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='374' text-anchor='end' fill='{accent}' font-size='46' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", label_x = content_x + 28, surface = palette.surface),
        2 => format!("<text x='540' y='356' text-anchor='middle' fill='{accent}' font-size='28' font-weight='700' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='540' y='430' text-anchor='middle' fill='{text}' font-size='58' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", text = palette.text),
        3 => format!("<text x='{content_x}' y='360' fill='{muted}' font-size='26' letter-spacing='6' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><circle cx='{circle_x}' cy='354' r='42' fill='none' stroke='{accent}' stroke-width='4'/><text x='{circle_x}' y='372' text-anchor='middle' fill='{accent}' font-size='38' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", circle_x = content_right - 42, muted = palette.muted),
        4 => format!("<line x1='{content_x}' y1='350' x2='{line_end}' y2='350' stroke='{accent}' stroke-width='4'/><text x='{content_x}' y='400' fill='{muted}' font-size='24' letter-spacing='5' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><text x='{content_right}' y='400' text-anchor='end' fill='{accent}' font-size='48' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", line_end = content_x + 260, muted = palette.muted),
        _ => format!("<text x='{content_x}' y='365' fill='{accent}' font-size='24' letter-spacing='8' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{tag}</text><rect x='{rect_x}' y='320' width='112' height='76' rx='16' fill='{accent}' opacity='.18'/><text x='{content_right}' y='374' text-anchor='end' fill='{text}' font-size='44' font-weight='900' font-family='Noto Serif CJK SC, Noto Serif SC, serif'>{num}</text>", rect_x = content_right - 112, text = palette.text),
    }
}

fn designer_divider(
    seed: usize,
    content_x: u32,
    content_right: u32,
    line_y: u32,
    palette: DesignerPalette,
    accent: &str,
) -> String {
    match seed % 5 {
        0 => format!("<rect x='{content_x}' y='{line_y}' width='150' height='7' rx='3' fill='{accent}'/>"),
        1 => format!("<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='{accent}' stroke-opacity='.42' stroke-width='2' stroke-dasharray='16 12'/>"),
        2 => format!("<line x1='{content_x}' y1='{line_y}' x2='{content_right}' y2='{line_y}' stroke='{muted}' stroke-opacity='.42'/><circle cx='{content_x}' cy='{line_y}' r='6' fill='{accent}'/><circle cx='{content_right}' cy='{line_y}' r='6' fill='{accent}'/>", muted = palette.muted),
        3 => format!("<path d='M{content_x} {line_y}H{mid}L{mid2} {line_y}H{content_right}' stroke='{accent}' stroke-width='4' fill='none'/>", mid = content_x + 180, mid2 = content_x + 240),
        _ => format!("<rect x='{content_x}' y='{line_y}' width='{width}' height='2' fill='{muted}' opacity='.42'/><rect x='{content_x}' y='{bar_y}' width='90' height='8' rx='4' fill='{accent}'/>", width = content_right - content_x, bar_y = line_y - 3, muted = palette.muted),
    }
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

/// Applies user-selected typography after a template has been assembled.
/// Keeping this at the SVG boundary lets all templates share the same controls
/// without duplicating style plumbing in each design function.
fn apply_typography(mut svg: String, request: &RenderRequest) -> String {
    let family = match request.font_family.as_str() {
        "sans" | "system" => Some("Noto Sans CJK SC, Noto Sans SC, sans-serif"),
        "serif" => Some("Noto Serif CJK SC, Noto Serif SC, serif"),
        "mono" => Some("Noto Sans Mono, Noto Sans CJK SC, sans-serif"),
        _ => None,
    };
    let title_color = color_option(&request.title_color);
    let body_color = color_option(&request.body_color);
    let tag_color = color_option(&request.tag_color);
    let customized = family.is_some()
        || request.title_weight > 0
        || request.body_weight > 0
        || title_color.is_some()
        || body_color.is_some()
        || tag_color.is_some();

    let mut output = String::with_capacity(svg.len() + 64);
    let mut cursor = 0;
    while let Some(relative_start) = svg[cursor..].find("<text ") {
        let start = cursor + relative_start;
        output.push_str(&svg[cursor..start]);
        let Some(relative_end) = svg[start..].find('>') else {
            output.push_str(&svg[start..]);
            cursor = svg.len();
            break;
        };
        let opening_end = start + relative_end + 1;
        let closing = "</text>";
        let Some(relative_close) = svg[opening_end..].find(closing) else {
            output.push_str(&svg[start..]);
            cursor = svg.len();
            break;
        };
        let close_start = opening_end + relative_close;
        let opening = &svg[start..opening_end];
        let role = if opening.contains("data-role='title'") {
            Some("title")
        } else if opening.contains("data-role='body'") {
            Some("body")
        } else if opening.contains("data-role='tag'") {
            Some("tag")
        } else {
            None
        };
        let mut rewritten = opening.to_string();
        if let Some(family) = family {
            replace_svg_attribute(&mut rewritten, "font-family", family);
        }
        if let Some(role) = role {
            let weight = match role {
                "title" => request.title_weight,
                "body" => request.body_weight,
                _ => 0,
            };
            if weight > 0 {
                replace_svg_attribute(&mut rewritten, "font-weight", &weight.to_string());
            }
            let selected_color = match role {
                "title" => title_color,
                "body" => body_color,
                "tag" => tag_color,
                _ => None,
            };
            if let Some(selected_color) = selected_color {
                replace_svg_attribute(&mut rewritten, "fill", selected_color);
            }
        }
        if customized {
            rewritten = strip_role_marker(&rewritten);
        }
        output.push_str(&rewritten);
        output.push_str(&svg[opening_end..close_start + closing.len()]);
        cursor = close_start + closing.len();
    }
    if cursor < svg.len() {
        output.push_str(&svg[cursor..]);
    }
    if !customized {
        output = output
            .replace(" data-role='title'", "")
            .replace(" data-role='body'", "")
            .replace(" data-role='tag'", "");
    }
    svg = output;
    svg
}

fn color_option(value: &str) -> Option<&str> {
    let valid = (value.len() == 4 || value.len() == 7)
        && value.starts_with('#')
        && value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit());
    valid.then_some(value)
}

fn replace_svg_attribute(opening: &mut String, name: &str, value: &str) {
    let needle = format!("{name}='");
    let Some(value_start) = opening.find(&needle).map(|index| index + needle.len()) else {
        return;
    };
    let Some(relative_end) = opening[value_start..].find('\'') else {
        return;
    };
    let value_end = value_start + relative_end;
    opening.replace_range(value_start..value_end, value);
}

fn strip_role_marker(opening: &str) -> String {
    opening
        .replace(" data-role='title'", "")
        .replace(" data-role='body'", "")
        .replace(" data-role='tag'", "")
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
            format!("<text data-role='title' x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-weight='{weight}' font-family='{family}'>{}</text>", xml(line))
        })
        .collect::<Vec<_>>()
        .join("");
    (svg, last_baseline)
}

#[allow(clippy::too_many_arguments)]
fn body_lines(
    text: &str,
    max_chars: usize,
    size: u32,
    x: u32,
    y: u32,
    max_baseline: u32,
    fill: &str,
    family: &str,
    weight: u32,
    line_height: f32,
) -> String {
    let layout = body_layout(text, max_chars, size, y, max_baseline, line_height);
    let lines = if layout.truncated {
        limited_lines(text, layout.max_chars, layout.max_lines)
    } else {
        wrap_text(text, layout.max_chars)
    };
    let line_step = (layout.size as f32 * layout.line_height) as u32;
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let baseline = y + (index as u32 * line_step);
            format!("<text data-role='body' x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-weight='{weight}' font-family='{family}'>{}</text>", xml(line), size = layout.size)
        })
        .collect::<Vec<_>>()
        .join("")
}

struct BodyLayout {
    max_chars: usize,
    max_lines: usize,
    size: u32,
    line_height: f32,
    truncated: bool,
}

fn body_layout(
    text: &str,
    base_max_chars: usize,
    base_size: u32,
    y: u32,
    max_baseline: u32,
    base_line_height: f32,
) -> BodyLayout {
    let min_size = base_size.saturating_sub(8).max(36);
    let sizes = (min_size..=base_size)
        .rev()
        .filter(|size| (base_size - size) % 2 == 0);
    for size in sizes {
        let layout = candidate_body_layout(
            base_max_chars,
            base_size,
            size,
            y,
            max_baseline,
            base_line_height,
            false,
        );
        if wrap_text(text, layout.max_chars).len() <= layout.max_lines {
            return layout;
        }
    }
    for reduction in [0.06_f32, 0.12, 0.18] {
        let line_height = (base_line_height - reduction).max(1.35);
        let layout = candidate_body_layout(
            base_max_chars,
            base_size,
            min_size,
            y,
            max_baseline,
            line_height,
            false,
        );
        if wrap_text(text, layout.max_chars).len() <= layout.max_lines {
            return layout;
        }
    }
    candidate_body_layout(
        base_max_chars,
        base_size,
        min_size,
        y,
        max_baseline,
        (base_line_height - 0.18).max(1.35),
        true,
    )
}

fn candidate_body_layout(
    base_max_chars: usize,
    base_size: u32,
    size: u32,
    y: u32,
    max_baseline: u32,
    line_height: f32,
    truncated: bool,
) -> BodyLayout {
    let line_step = (size as f32 * line_height) as u32;
    let max_lines = (max_baseline.saturating_sub(y) / line_step + 1).max(1) as usize;
    let max_chars =
        ((base_max_chars as f32 * base_size as f32 / size as f32).floor() as usize).max(1);
    BodyLayout {
        max_chars,
        max_lines,
        size,
        line_height,
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
            format!("<text data-role='tag' x='{x}' y='{baseline}' fill='{fill}' font-size='{size}' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{}</text>", xml(line))
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
        result.push_str(&format!("<rect x='{cursor}' y='{row_y}' width='{width}' height='52' rx='20' fill='{fill}'/><text data-role='tag' x='{}' y='{}' fill='#FFFFFF' font-size='26' font-family='Noto Sans CJK SC, Noto Sans SC, sans-serif'>{}</text>", cursor + 20, row_y + 35, xml(&label)));
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

    const ALL_TEMPLATES: &[&str] = &[
        "magazine",
        "magazine_pro",
        "fresh",
        "minimal",
        "poster",
        "journal",
        "neon",
        "newspaper",
        "blueprint",
        "polaroid",
        "scoreboard",
        "vaporwave",
        "classic",
        "mono",
        "club",
        "street",
        "aurora",
        "espresso",
        "sunset",
        "ocean",
        "rainforest",
        "chalkboard",
        "luxury",
        "arcade",
        "comic",
        "paper_cut",
        "gradient_card",
        "ink_brush",
        "tech_hud",
        "warm_note",
        "ice",
        "candy",
        "carbon",
        "ceramic",
        "matchday",
        "lounge",
        "diagonal",
        "retro_tv",
        "terrazzo",
        "silk",
        "desert",
        "mint",
        "royal_blue",
        "darkroom",
        "data_panel",
        "sticker_bomb",
        "prism",
        "stadium",
        "typewriter",
        "zen",
    ];

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
    fn adapts_body_text_before_truncating() {
        let text = "一".repeat(48);
        let layout = body_layout(&text, 10, 48, 100, 337, 1.65);
        assert!(layout.size < 48);
        assert!(!layout.truncated);
    }

    #[test]
    fn truncates_only_after_adaptive_limits() {
        let text = "一".repeat(300);
        let layout = body_layout(&text, 10, 48, 100, 337, 1.65);
        assert_eq!(layout.size, 40);
        assert!(layout.truncated);
    }

    #[test]
    fn all_templates_emit_canvas() {
        let request = RenderRequest {
            title: "测试".into(),
            body: "正文".into(),
            ..Default::default()
        };
        for template in ALL_TEMPLATES {
            let mut request = request.clone();
            request.template = (*template).into();
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
            11_180_669_618_038_991_886,
            4_903_366_455_419_328_711,
            2_606_950_486_791_333_417,
            8_051_477_551_424_268_749,
            5_231_298_719_142_288_172,
            10_728_926_279_769_676_032,
            664_092_054_982_800_083,
            405_228_027_209_619_123,
            13_636_155_439_905_564_751,
            10_651_597_118_353_426_562,
            13_047_415_745_521_056_090,
            425_179_590_397_066_224,
            18_253_373_370_898_788_892,
            11_216_347_276_862_073_211,
            13_730_218_183_808_770_834,
            6_430_787_066_683_608_935,
            3_234_824_405_208_935_073,
            18_187_517_618_597_569_123,
            1_805_943_929_175_856_004,
            12_996_206_187_188_395_826,
            5_569_554_980_941_469_596,
            13_162_614_678_469_734_302,
            15_852_542_926_123_123_017,
            18_339_905_956_085_081_110,
            8_775_783_983_319_563_219,
            11_199_509_825_177_981_171,
            4_356_646_044_146_357_423,
            16_321_755_061_417_390_432,
            18_372_474_689_725_090_887,
            6_741_746_859_477_128_882,
            14_859_183_518_072_843_950,
            10_117_094_781_850_269_608,
            4_824_968_417_762_490_383,
            12_074_108_589_338_291_216,
            14_523_749_511_889_669_713,
            4_440_718_292_068_059_524,
            16_069_936_373_001_067_402,
            2_272_785_499_300_880_258,
            4_293_071_393_310_320_814,
            849_866_747_646_345_396,
            5_244_810_571_438_347_945,
            11_113_663_352_077_584_739,
            15_611_396_146_798_677_209,
            9_296_320_271_288_400_569,
        ];
        let actual = ALL_TEMPLATES
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
    fn applies_typography_overrides_to_rendered_roles() {
        let request = RenderRequest {
            title: "标题样式".into(),
            body: "正文样式".into(),
            tags: "#话题".into(),
            font_family: "serif".into(),
            title_weight: 900,
            body_weight: 300,
            title_color: "#112233".into(),
            body_color: "#223344".into(),
            tag_color: "#334455".into(),
            ..Default::default()
        };
        let svg = svg_for(&request);
        assert!(!svg.contains("data-role="));
        assert!(svg.contains("font-family='Noto Serif CJK SC, Noto Serif SC, serif'"));
        assert!(svg.contains("fill='#112233' font-size"));
        assert!(svg.contains("fill='#223344' font-size"));
        assert!(svg.contains("fill='#334455' font-size"));
        assert!(svg.contains("font-weight='900'"));
        assert!(svg.contains("font-weight='300'"));
    }

    #[test]
    #[ignore = "manual template alignment visual review"]
    fn writes_template_alignment_samples() {
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/template-alignment-review");
        fs::create_dir_all(&directory).expect("create visual review directory");
        for template in ALL_TEMPLATES {
            let request = RenderRequest {
                template: (*template).into(),
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
                template: ALL_TEMPLATES[index % ALL_TEMPLATES.len()].into(),
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
