use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderRequest {
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default = "default_num")]
    pub num: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default = "default_glow1")]
    pub glow1: String,
    #[serde(default = "default_glow2")]
    pub glow2: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default)]
    pub subfolder: String,
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            template: default_template(),
            num: default_num(),
            tag: "BILLIARDS".into(),
            title: String::new(),
            body: String::new(),
            tags: String::new(),
            glow1: default_glow1(),
            glow2: default_glow2(),
            accent: default_accent(),
            subfolder: String::new(),
        }
    }
}

fn default_template() -> String {
    "magazine".into()
}
fn default_num() -> String {
    "01".into()
}
fn default_glow1() -> String {
    "#FF8A5C".into()
}
fn default_glow2() -> String {
    "#FF5E62".into()
}
fn default_accent() -> String {
    "#FF5E62".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResponse {
    pub image_base64: String,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "copyLimits")]
    pub copy_limits: CopyFitLimits,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFitLimits {
    pub title_chars: usize,
    pub body_chars: usize,
    pub body_lines: usize,
    pub tags_count: usize,
    pub tag_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub name: String,
    pub tag: String,
    pub glow1: String,
    pub glow2: String,
    pub accent: String,
    pub title: String,
    pub body: String,
    pub tags: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub completed: usize,
    pub total: usize,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailure {
    pub completed: usize,
    pub total: usize,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobComplete {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub template: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(default)]
    pub cloud_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub status: String,
}

fn default_level() -> String {
    "2档".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyItem {
    #[serde(default)]
    pub cloud_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SettingsInput {
    pub api_url: Option<String>,
    pub api_key: Option<String>,
    pub api_model: Option<String>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub server_url: String,
    pub server_configured: bool,
    pub logged_in: bool,
    pub email: String,
    pub last_sync_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncResult {
    pub direction: String,
    pub accounts: usize,
    pub copy_items: usize,
    pub synced_at: i64,
}
