use crate::{
    models::{LibraryEntry, LibraryPage},
    storage,
};
use rusqlite::{params, Connection};
use serde_json::Value;
use tauri::AppHandle;

fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS ui_workspace (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS render_checkpoints (job_id TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS library_labels (title TEXT NOT NULL, body TEXT NOT NULL, tags TEXT NOT NULL, category TEXT NOT NULL DEFAULT '', favorite INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(title,body,tags));")
        .map_err(|error| error.to_string())
}
fn connect(app: &AppHandle) -> Result<Connection, String> {
    let conn = storage::connection(app)?;
    initialize(&conn)?;
    Ok(conn)
}

#[tauri::command]
pub fn save_workspace(app: AppHandle, workspace: Value) -> Result<(), String> {
    validate_workspace(&workspace)?;
    let conn = connect(&app)?;
    conn.execute("INSERT INTO ui_workspace(id,value) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value", [workspace.to_string()]).map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_workspace(workspace: &Value) -> Result<(), String> {
    let object = workspace.as_object().ok_or("草稿格式无效")?;
    if workspace.to_string().len() > 2 * 1024 * 1024 {
        return Err("草稿超过 2 MB".into());
    }
    if object.keys().any(|key| {
        ![
            "version",
            "fields",
            "batch",
            "favorites",
            "presets",
            "mix",
            "pendingRender",
            "palette",
            "updatedAt",
        ]
        .contains(&key.as_str())
    }) {
        return Err("草稿包含不支持的字段".into());
    }
    if let Some(fields) = workspace.get("fields").and_then(Value::as_object) {
        let allowed = include_str!("../../frontend/workspace-fields.json");
        let ids: Vec<String> = serde_json::from_str(allowed).map_err(|error| error.to_string())?;
        if fields.keys().any(|key| !ids.contains(key)) {
            return Err("草稿字段不在允许列表中，账号密码与密钥不能写入草稿".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn load_workspace(app: AppHandle) -> Result<Option<Value>, String> {
    let conn = connect(&app)?;
    use rusqlite::OptionalExtension;
    let raw: Option<String> = conn
        .query_row("SELECT value FROM ui_workspace WHERE id=1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| error.to_string())?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
        .transpose()
}

fn list(
    conn: &Connection,
    query: &str,
    category: &str,
    favorites_only: bool,
    offset: usize,
    limit: usize,
) -> Result<LibraryPage, String> {
    let filter="FROM copy_library c LEFT JOIN library_labels l ON l.title=c.title AND l.body=c.body AND l.tags=c.tags WHERE (?1='' OR instr(lower(c.title||char(10)||c.body||char(10)||c.tags),lower(?1))>0) AND (?2='' OR coalesce(l.category,'')=?2) AND (?3=0 OR coalesce(l.favorite,0)=1)";
    let total: i64 = conn
        .query_row(
            &format!("SELECT count(*) {filter}"),
            params![query, category, favorites_only],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let mut statement=conn.prepare(&format!("SELECT c.id,c.title,c.body,c.tags,coalesce(l.category,''),coalesce(l.favorite,0) {filter} ORDER BY coalesce(l.favorite,0) DESC,c.id DESC LIMIT ?4 OFFSET ?5")).map_err(|error|error.to_string())?;
    let entries = statement
        .query_map(
            params![query, category, favorites_only, limit as i64, offset as i64],
            |row| {
                Ok(LibraryEntry {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    tags: row.get(3)?,
                    category: row.get(4)?,
                    favorite: row.get(5)?,
                })
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(LibraryPage {
        entries,
        total: total as usize,
    })
}

#[tauri::command]
pub fn list_library_entries(
    app: AppHandle,
    query: Option<String>,
    category: Option<String>,
    favorites_only: Option<bool>,
    offset: Option<usize>,
) -> Result<LibraryPage, String> {
    list(
        &connect(&app)?,
        query.as_deref().unwrap_or(""),
        category.as_deref().unwrap_or(""),
        favorites_only.unwrap_or(false),
        offset.unwrap_or(0),
        50,
    )
}

fn update(conn: &mut Connection, entry: &LibraryEntry) -> Result<(), String> {
    if entry.title.trim().is_empty()
        || entry.body.trim().is_empty()
        || entry.title.chars().count() > 1000
        || entry.body.chars().count() > 10000
        || entry.tags.len() > 2000
        || entry.category.chars().count() > 30
    {
        return Err("文案字段为空或过长".into());
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let count = tx
        .execute(
            "UPDATE copy_library SET title=?1,body=?2,tags=?3 WHERE id=?4",
            params![entry.title, entry.body, entry.tags, entry.id],
        )
        .map_err(|error| error.to_string())?;
    if count != 1 {
        return Err("文案已删除，请刷新列表".into());
    }
    tx.execute("INSERT INTO library_labels(title,body,tags,category,favorite) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(title,body,tags) DO UPDATE SET category=excluded.category,favorite=excluded.favorite",params![entry.title,entry.body,entry.tags,entry.category,entry.favorite]).map_err(|error|error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_library_entry(app: AppHandle, entry: LibraryEntry) -> Result<(), String> {
    update(&mut connect(&app)?, &entry)
}

#[tauri::command]
pub fn delete_library_entry(app: AppHandle, id: i64) -> Result<(), String> {
    let conn = connect(&app)?;
    conn.execute("DELETE FROM copy_library WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn csv_cell(text: &str) -> String {
    let text = if text.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    format!("\"{}\"", text.replace('"', "\"\""))
}

#[tauri::command]
pub fn export_library_entries(
    app: AppHandle,
    query: Option<String>,
    category: Option<String>,
    favorites_only: Option<bool>,
) -> Result<String, String> {
    let conn = connect(&app)?;
    let page = list(
        &conn,
        query.as_deref().unwrap_or(""),
        category.as_deref().unwrap_or(""),
        favorites_only.unwrap_or(false),
        0,
        10000,
    )?;
    if page.total > 10000 {
        return Err("筛选结果超过 10000 条，请缩小范围再导出".into());
    }
    if page.total == 0 {
        return Err("当前筛选没有文案".into());
    }
    let dir = storage::output_dir(&app)?.join(format!("文案导出_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut csv = String::from("\u{feff}标题,正文,话题,分类,收藏\r\n");
    for entry in &page.entries {
        csv.push_str(&format!(
            "{},{},{},{},{}\r\n",
            csv_cell(&entry.title),
            csv_cell(&entry.body),
            csv_cell(&entry.tags),
            csv_cell(&entry.category),
            entry.favorite
        ));
    }
    std::fs::write(dir.join("文案.csv"), csv).map_err(|error| error.to_string())?;
    std::fs::write(
        dir.join("文案.json"),
        serde_json::to_vec_pretty(&page.entries).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

pub fn save_render_checkpoint(app: &AppHandle, job_id: &str, value: Value) -> Result<(), String> {
    let conn = connect(app)?;
    conn.execute("INSERT INTO render_checkpoints(job_id,value) VALUES(?1,?2) ON CONFLICT(job_id) DO UPDATE SET value=excluded.value",params![job_id,value.to_string()]).map_err(|error|error.to_string())?;
    conn.execute("DELETE FROM render_checkpoints WHERE job_id NOT IN (SELECT job_id FROM render_checkpoints ORDER BY rowid DESC LIMIT 20)",[]).map_err(|error|error.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn load_render_checkpoint(app: AppHandle, job_id: String) -> Result<Option<Value>, String> {
    use rusqlite::OptionalExtension;
    let conn = connect(&app)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM render_checkpoints WHERE job_id=?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_survive_row_recreation_and_filters_find_all_entries() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE copy_library(id INTEGER PRIMARY KEY,title TEXT,body TEXT,tags TEXT); INSERT INTO copy_library VALUES(1,'初次参赛','想一起去','台球');").unwrap();
        initialize(&conn).unwrap();
        update(
            &mut conn,
            &LibraryEntry {
                id: 1,
                title: "初次参赛".into(),
                body: "想一起去".into(),
                tags: "台球".into(),
                category: "找搭子".into(),
                favorite: true,
            },
        )
        .unwrap();
        conn.execute_batch("DELETE FROM copy_library; INSERT INTO copy_library VALUES(9,'初次参赛','想一起去','台球');").unwrap();
        let page = list(&conn, "参赛", "找搭子", true, 0, 50).unwrap();
        assert_eq!(page.total, 1);
        assert!(page.entries[0].favorite);
        assert_eq!(list(&conn, "不存在", "", false, 0, 50).unwrap().total, 0);
    }
    #[test]
    fn drafts_cannot_capture_password_or_key_fields() {
        assert!(
            validate_workspace(&serde_json::json!({"fields":{"localAiKey":"secret"}})).is_err()
        );
        assert!(
            validate_workspace(&serde_json::json!({"version":1,"fields":{"title":"找搭子"}}))
                .is_ok()
        );
    }
}
