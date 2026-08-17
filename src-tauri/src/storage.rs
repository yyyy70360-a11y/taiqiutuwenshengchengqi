use crate::models::{Account, CopyItem, HistoryEntry, PresetInfo, SettingsInput};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

const KEYCHAIN_SERVICE: &str = "com.billiards.matrix";
const KEYCHAIN_USER: &str = "api_key";

pub fn connection(app: &AppHandle) -> Result<Connection, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {error}"))?;
    fs::create_dir_all(&directory).map_err(|error| format!("创建应用数据目录失败: {error}"))?;
    let path = directory.join("billiards.sqlite3");
    let conn = Connection::open(path).map_err(|error| format!("打开数据库失败: {error}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS accounts (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, level TEXT NOT NULL, region TEXT NOT NULL, persona TEXT NOT NULL, tone TEXT NOT NULL, status TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS copy_library (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, body TEXT NOT NULL, tags TEXT NOT NULL, created_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS render_history (id INTEGER PRIMARY KEY AUTOINCREMENT, file_name TEXT NOT NULL, template TEXT NOT NULL, created_at INTEGER NOT NULL, output_dir TEXT NOT NULL DEFAULT '');
         CREATE TABLE IF NOT EXISTS presets (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, tag TEXT NOT NULL, glow1 TEXT NOT NULL, glow2 TEXT NOT NULL, accent TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, tags TEXT NOT NULL);",
    )
    .map_err(|error| format!("初始化数据库失败: {error}"))?;
    let has_output_dir: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('render_history') WHERE name = 'output_dir')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("检查历史数据结构失败: {error}"))?;
    if !has_output_dir {
        conn.execute(
            "ALTER TABLE render_history ADD COLUMN output_dir TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|error| format!("升级历史数据结构失败: {error}"))?;
    }
    seed_presets(&conn)?;
    Ok(conn)
}

pub fn get_presets(app: &AppHandle) -> Result<Vec<PresetInfo>, String> {
    let conn = connection(app)?;
    let mut statement = conn
        .prepare(
            "SELECT name, tag, glow1, glow2, accent, title, body, tags FROM presets ORDER BY id",
        )
        .map_err(|error| format!("读取预设失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(PresetInfo {
                name: row.get(0)?,
                tag: row.get(1)?,
                glow1: row.get(2)?,
                glow2: row.get(3)?,
                accent: row.get(4)?,
                title: row.get(5)?,
                body: row.get(6)?,
                tags: row.get(7)?,
            })
        })
        .map_err(|error| format!("读取预设失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取预设失败: {error}")))
        .collect()
}

pub fn ai_config(app: &AppHandle) -> Result<(String, String, String), String> {
    let conn = connection(app)?;
    let url = read_setting(&conn, "api_url")?
        .unwrap_or_else(|| "https://api.deepseek.com/v1/chat/completions".into());
    let model = read_setting(&conn, "api_model")?.unwrap_or_else(|| "deepseek-chat".into());
    let key = read_api_key()?.unwrap_or_default();
    Ok((url, key, model))
}

pub fn get_settings(app: &AppHandle) -> Result<Value, String> {
    let conn = connection(app)?;
    let url = read_setting(&conn, "api_url")?
        .unwrap_or_else(|| "https://api.deepseek.com/v1/chat/completions".into());
    let model = read_setting(&conn, "api_model")?.unwrap_or_else(|| "deepseek-chat".into());
    let api_key_configured =
        read_setting(&conn, "api_key_configured")?.and_then(|value| stored_api_key_status(&value));
    let output_dir = read_setting(&conn, "output_dir")?.unwrap_or_else(|| {
        app.path()
            .app_data_dir()
            .map(|path| path.join("output"))
            .unwrap_or_else(|_| PathBuf::from("output"))
            .to_string_lossy()
            .to_string()
    });
    Ok(json!({
        "api_url": url,
        "api_model": model,
        "api_key_configured": api_key_configured,
        "output_dir": output_dir
    }))
}

pub fn get_api_key_status(app: &AppHandle) -> Result<bool, String> {
    let configured = read_api_key()?.is_some();
    let conn = connection(app)?;
    write_setting(
        &conn,
        "api_key_configured",
        if configured { "true" } else { "false" },
    )?;
    Ok(configured)
}

pub fn output_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let conn = connection(app)?;
    if let Some(value) = read_setting(&conn, "output_dir")? {
        if !value.trim().is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {error}"))?
        .join("output"))
}

pub fn set_settings(app: &AppHandle, settings: SettingsInput) -> Result<(), String> {
    let conn = connection(app)?;
    if let Some(value) = settings.api_url.filter(|value| !value.trim().is_empty()) {
        write_setting(&conn, "api_url", &value)?;
    }
    if let Some(value) = settings.api_model.filter(|value| !value.trim().is_empty()) {
        write_setting(&conn, "api_model", &value)?;
    }
    if let Some(value) = settings.output_dir.filter(|value| !value.trim().is_empty()) {
        validate_output_dir(&PathBuf::from(&value))?;
        write_setting(&conn, "output_dir", &value)?;
    }
    if let Some(value) = settings.api_key.filter(|value| !value.trim().is_empty()) {
        keychain_entry()?
            .set_password(&value)
            .map_err(|error| format!("保存 API Key 失败: {error}"))?;
        write_setting(&conn, "api_key_configured", "true")?;
    }
    Ok(())
}

pub fn get_accounts(app: &AppHandle) -> Result<Vec<Account>, String> {
    let conn = connection(app)?;
    let mut statement = conn
        .prepare("SELECT name, level, region, persona, tone, status FROM accounts ORDER BY id")
        .map_err(|error| format!("读取账号失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(Account {
                name: row.get(0)?,
                level: row.get(1)?,
                region: row.get(2)?,
                persona: row.get(3)?,
                tone: row.get(4)?,
                status: row.get(5)?,
            })
        })
        .map_err(|error| format!("读取账号失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取账号失败: {error}")))
        .collect()
}

pub fn save_accounts(app: &AppHandle, accounts: Vec<Account>) -> Result<(), String> {
    let mut conn = connection(app)?;
    let transaction = conn
        .transaction()
        .map_err(|error| format!("开始账号事务失败: {error}"))?;
    transaction
        .execute("DELETE FROM accounts", [])
        .map_err(|error| format!("清空账号失败: {error}"))?;
    for account in accounts {
        transaction.execute("INSERT INTO accounts (name, level, region, persona, tone, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![account.name, account.level, account.region, account.persona, account.tone, account.status]).map_err(|error| format!("保存账号失败: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交账号事务失败: {error}"))
}

pub fn get_library(app: &AppHandle) -> Result<Vec<CopyItem>, String> {
    let conn = connection(app)?;
    let mut statement = conn
        .prepare("SELECT title, body, tags FROM copy_library ORDER BY id DESC LIMIT 200")
        .map_err(|error| format!("读取文案库失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(CopyItem {
                title: row.get(0)?,
                body: row.get(1)?,
                tags: row.get(2)?,
            })
        })
        .map_err(|error| format!("读取文案库失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取文案库失败: {error}")))
        .collect()
}

pub fn save_library_item(app: &AppHandle, item: CopyItem) -> Result<(), String> {
    let conn = connection(app)?;
    conn.execute("INSERT INTO copy_library (title, body, tags, created_at) VALUES (?1, ?2, ?3, strftime('%s','now'))", params![item.title, item.body, item.tags])
        .map_err(|error| format!("保存文案失败: {error}"))?;
    Ok(())
}

fn import_library_file(conn: &mut Connection, items: Vec<CopyItem>) -> Result<usize, String> {
    let transaction = conn
        .transaction()
        .map_err(|error| format!("开始旧文案库迁移事务失败: {error}"))?;
    let mut imported = 0;
    for item in items {
        if item.title.trim().is_empty() || item.body.trim().is_empty() {
            continue;
        }
        transaction
            .execute(
                "INSERT INTO copy_library (title, body, tags, created_at) VALUES (?1, ?2, ?3, strftime('%s','now'))",
                params![item.title, item.body, item.tags],
            )
            .map_err(|error| format!("迁移旧文案失败: {error}"))?;
        imported += 1;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交旧文案库迁移事务失败: {error}"))?;
    Ok(imported)
}

pub fn import_legacy_library(app: &AppHandle, items: Vec<CopyItem>) -> Result<usize, String> {
    let mut conn = connection(app)?;
    if read_setting(&conn, "legacy_localstorage_library_v1")?.as_deref() == Some("complete") {
        return Ok(0);
    }
    let transaction = conn
        .transaction()
        .map_err(|error| format!("开始文案库迁移事务失败: {error}"))?;
    let mut imported = 0;
    for item in items.into_iter().take(200) {
        if item.title.trim().is_empty() || item.body.trim().is_empty() {
            continue;
        }
        imported += transaction
            .execute(
                "INSERT INTO copy_library (title, body, tags, created_at)
                 SELECT ?1, ?2, ?3, strftime('%s','now')
                 WHERE NOT EXISTS (
                    SELECT 1 FROM copy_library WHERE title = ?1 AND body = ?2 AND tags = ?3
                 )",
                params![item.title, item.body, item.tags],
            )
            .map_err(|error| format!("迁移旧文案失败: {error}"))?;
    }
    transaction
        .execute(
            "INSERT INTO settings (key, value) VALUES ('legacy_localstorage_library_v1', 'complete')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )
        .map_err(|error| format!("保存文案库迁移状态失败: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("提交文案库迁移失败: {error}"))?;
    Ok(imported)
}

pub fn record_history(
    app: &AppHandle,
    file_name: &str,
    template: &str,
    output_dir: &Path,
) -> Result<(), String> {
    let conn = connection(app)?;
    conn.execute("INSERT INTO render_history (file_name, template, created_at, output_dir) VALUES (?1, ?2, strftime('%s','now'), ?3)", params![file_name, template, output_dir.to_string_lossy()])
        .map_err(|error| format!("保存渲染历史失败: {error}"))?;
    Ok(())
}

pub fn get_history(app: &AppHandle) -> Result<Vec<HistoryEntry>, String> {
    let conn = connection(app)?;
    let mut statement = conn
        .prepare(
            "SELECT id, file_name, template, created_at FROM render_history ORDER BY id DESC LIMIT 50",
        )
        .map_err(|error| format!("读取渲染历史失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                file_name: row.get(1)?,
                template: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|error| format!("读取渲染历史失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取渲染历史失败: {error}")))
        .collect()
}

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    connection(app)?
        .execute("DELETE FROM render_history", [])
        .map_err(|error| format!("清空渲染历史失败: {error}"))?;
    Ok(())
}

pub fn history_location(app: &AppHandle, history_id: i64) -> Result<(PathBuf, String), String> {
    let conn = connection(app)?;
    let (file_name, saved_output_dir): (String, String) = conn
        .query_row(
            "SELECT file_name, output_dir FROM render_history WHERE id = ?1",
            [history_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => "历史记录不存在".to_string(),
            _ => format!("读取历史记录失败: {error}"),
        })?;
    let directory = if saved_output_dir.trim().is_empty() {
        output_dir(app)?
    } else {
        PathBuf::from(saved_output_dir)
    };
    Ok((directory, file_name))
}

pub fn migrate_legacy(app: &AppHandle) -> Result<Value, String> {
    let mut conn = connection(app)?;
    let mut roots = legacy_roots(app);
    let mut seen = HashSet::new();
    roots.retain(|path| seen.insert(path.clone()));
    let migration_complete =
        read_setting(&conn, "legacy_migration_v1")?.as_deref() == Some("complete");
    let secrets_scrubbed =
        read_setting(&conn, "legacy_secrets_scrubbed_v1")?.as_deref() == Some("complete");
    if migration_complete && secrets_scrubbed {
        return Ok(json!({ "status": "already_complete", "imported": [] }));
    }

    let mut imported = Vec::new();
    for root in &roots {
        let settings_path = root.join("settings.json");
        if settings_path.is_file() {
            let value: Value = serde_json::from_str(
                &fs::read_to_string(&settings_path)
                    .map_err(|error| format!("读取旧设置失败: {error}"))?,
            )
            .map_err(|error| format!("解析旧设置失败: {error}"))?;
            import_settings_if_missing(&conn, &value)?;
            imported.push("settings");
        }
        let accounts_path = root.join("accounts.json");
        if accounts_path.is_file() && table_is_empty(&conn, "accounts")? {
            let accounts: Vec<Account> = serde_json::from_str(
                &fs::read_to_string(&accounts_path)
                    .map_err(|error| format!("读取旧账号失败: {error}"))?,
            )
            .map_err(|error| format!("解析旧账号失败: {error}"))?;
            save_accounts(app, accounts)?;
            imported.push("accounts");
        }
        if table_is_empty(&conn, "copy_library")? {
            for name in ["copy_library.json", "library.json", "copies.json"] {
                let library_path = root.join(name);
                if !library_path.is_file() {
                    continue;
                }
                let items: Vec<CopyItem> = serde_json::from_str(
                    &fs::read_to_string(&library_path)
                        .map_err(|error| format!("读取旧文案库失败: {error}"))?,
                )
                .map_err(|error| format!("解析旧文案库失败: {error}"))?;
                import_library_file(&mut conn, items)?;
                imported.push("copy_library");
                break;
            }
        }
    }

    write_setting(&conn, "legacy_migration_v1", "complete")?;
    scrub_legacy_keys(&roots)?;
    write_setting(&conn, "legacy_secrets_scrubbed_v1", "complete")?;
    Ok(json!({
        "status": if imported.is_empty() { "not_found" } else { "complete" },
        "imported": imported
    }))
}

fn scrub_legacy_keys(roots: &[PathBuf]) -> Result<(), String> {
    for root in roots {
        let path = root.join("settings.json");
        if !path.is_file() {
            continue;
        }
        let mut value: Value = serde_json::from_str(
            &fs::read_to_string(&path)
                .map_err(|error| format!("读取旧设置清理状态失败: {error}"))?,
        )
        .map_err(|error| format!("解析旧设置清理状态失败: {error}"))?;
        let Some(object) = value.as_object_mut() else {
            return Err("旧设置格式无效，无法清理明文 API Key".into());
        };
        let removed_api_key = object.remove("api_key").is_some();
        let removed_legacy_key = object.remove("deepseek_key").is_some();
        let changed = removed_api_key || removed_legacy_key;
        if !changed {
            continue;
        }
        let temporary = path.with_extension("json.migrating");
        let bytes = serde_json::to_vec_pretty(&value)
            .map_err(|error| format!("序列化旧设置失败: {error}"))?;
        fs::write(&temporary, bytes).map_err(|error| format!("写入旧设置清理结果失败: {error}"))?;
        fs::rename(&temporary, &path).map_err(|error| format!("替换旧设置失败: {error}"))?;
    }
    Ok(())
}

fn seed_presets(conn: &Connection) -> Result<(), String> {
    if !table_is_empty(conn, "presets")? {
        return Ok(());
    }
    let presets = [
        ("找搭子", "BILLIARDS", "#FF8A5C", "#FF5E62", "#FF5E62", "斗门的兄弟今晚有空吗", "井岸镇这边，粤尚CC或者响袋都行，2档求带。\n一个人打实在没劲，群里好几个今晚都去。\n7点到11点，不会打的也别怕，有新手一起。\n想来的滴滴", "#珠海台球 #珠海约球 #斗门台球 #台球搭子"),
        ("球房测评", "REVIEW", "#43E97B", "#38F9D7", "#0FB9B1", "香洲这家球房台子维护是真顶", "柠溪路YY俱乐部，乔氏台78一小时不便宜\n但台呢打理得干净，走位稳\n平时下班去两小时基本不用等，老板懂球会帮你调杆\n缺点周末人多吵，想认真打的工作日去", "#珠海台球 #香洲台球 #球房推荐"),
        ("新手避坑", "GUIDE", "#4FACFE", "#00C6FB", "#1E90FF", "学台球一个月花3000的血泪教训", "1.别上来买贵杆，我第一根800的现在吃灰，租着打先\n2.姿势比杆重要，错了改好久\n3.别跟高手赌球，纯送钱还打击信心\n4.找同水平搭子一起练比瞎打强", "#珠海台球 #台球新手 #金湾台球"),
        ("约球吐槽", "RANT", "#A18CD1", "#FBC2EB", "#8E44AD", "约球最烦的五种人你遇过几个", "1.约好放鸽子台子开了人没来\n2.一杆磨两分钟急死\n3.输了甩脸赢了得瑟\n4.占台不付钱上厕所半小时", "#珠海台球 #拱北台球 #台球搭子"),
        ("老炮说杆", "PRO TIP", "#2C3E50", "#4CA1AF", "#3A6E8F", "300和3000的杆3档以下没区别", "前台球房员工说句实话\n3档以下手感分不出杆好坏\n新手选杆三点：19盎司、11.5mm皮头、前节偏硬\n300块入门杆够用", "#珠海台球 #台球杆 #台球装备"),
    ];
    for preset in presets {
        conn.execute(
            "INSERT INTO presets (name, tag, glow1, glow2, accent, title, body, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![preset.0, preset.1, preset.2, preset.3, preset.4, preset.5, preset.6, preset.7],
        )
        .map_err(|error| format!("初始化预设失败: {error}"))?;
    }
    Ok(())
}

fn table_is_empty(conn: &Connection, table: &str) -> Result<bool, String> {
    let sql = match table {
        "accounts" => "SELECT NOT EXISTS(SELECT 1 FROM accounts)",
        "copy_library" => "SELECT NOT EXISTS(SELECT 1 FROM copy_library)",
        "presets" => "SELECT NOT EXISTS(SELECT 1 FROM presets)",
        _ => return Err("未知数据表".into()),
    };
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(|error| format!("检查数据表失败: {error}"))
}

fn validate_output_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("输出目录不可创建: {error}"))?;
    let probe = path.join(format!(".billiards-write-test-{}", std::process::id()));
    fs::write(&probe, b"ok").map_err(|error| format!("输出目录不可写: {error}"))?;
    fs::remove_file(&probe).map_err(|error| format!("输出目录测试文件清理失败: {error}"))
}

fn legacy_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current) = std::env::current_dir() {
        roots.push(current.clone());
        roots.push(current.join("workflow"));
        roots.push(current.join("../workflow"));
    }
    if let Ok(home) = app.path().home_dir() {
        roots.push(home.join("Documents/台球图文生成器"));
        roots.push(home.join("Documents/台球矩阵搭建/workflow"));
    }
    if let Ok(executable) = std::env::current_exe() {
        for ancestor in executable.ancestors().take(6) {
            roots.push(ancestor.join("workflow"));
        }
    }
    roots
}

fn import_settings_if_missing(conn: &Connection, value: &Value) -> Result<(), String> {
    for (key, legacy_key) in [
        ("api_url", "api_url"),
        ("api_model", "api_model"),
        ("output_dir", "output_dir"),
    ] {
        if read_setting(conn, key)?.is_none() {
            if let Some(setting) = value.get(legacy_key).and_then(Value::as_str) {
                if !setting.trim().is_empty() {
                    if key == "output_dir" {
                        validate_output_dir(&PathBuf::from(setting))?;
                    }
                    write_setting(conn, key, setting)?;
                }
            }
        }
    }
    if let Some(key) = value
        .get("api_key")
        .or_else(|| value.get("deepseek_key"))
        .and_then(Value::as_str)
        .filter(|key| !key.trim().is_empty())
    {
        let entry = keychain_entry()?;
        if read_api_key()?.is_none() {
            entry
                .set_password(key)
                .map_err(|error| format!("迁移 API Key 到系统钥匙串失败: {error}"))?;
        }
        write_setting(conn, "api_key_configured", "true")?;
    }
    Ok(())
}

fn keychain_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER)
        .map_err(|error| format!("访问系统钥匙串失败: {error}"))
}

fn read_api_key() -> Result<Option<String>, String> {
    match keychain_entry()?.get_password() {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("读取系统钥匙串失败: {error}")),
    }
}

fn stored_api_key_status(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn read_setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .map(Some)
    .or_else(|error| {
        if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
            Ok(None)
        } else {
            Err(format!("读取设置失败: {error}"))
        }
    })
}

fn write_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value])
        .map_err(|error| format!("保存设置失败: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_api_key_status_rejects_unknown_values() {
        assert_eq!(stored_api_key_status("true"), Some(true));
        assert_eq!(stored_api_key_status("false"), Some(false));
        assert_eq!(stored_api_key_status("configured"), None);
    }

    #[test]
    fn legacy_library_file_import_rolls_back_on_failure() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE copy_library (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL CHECK(title <> 'reject'),
                body TEXT NOT NULL,
                tags TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
        )
        .expect("create test table");
        let result = import_library_file(
            &mut conn,
            vec![
                CopyItem {
                    title: "first".into(),
                    body: "body".into(),
                    tags: "#one".into(),
                },
                CopyItem {
                    title: "reject".into(),
                    body: "body".into(),
                    tags: "#two".into(),
                },
            ],
        );
        assert!(result.is_err());
        let count: i64 = conn
            .query_row("SELECT count(*) FROM copy_library", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 0);
    }
}
