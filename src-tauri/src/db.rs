//! 本地 SQLite 存储（收藏 / 观看历史 / 应用设置）
//!
//! 替代 localStorage：数据保存在 app_local_data_dir/streamflow.db。
//! 收藏与历史整表替换（数据量小、语义简单）；设置按 key upsert。

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::sync::OnceCell;

static POOL: OnceCell<SqlitePool> = OnceCell::const_new();

pub async fn init(db_path: &str) -> Result<(), String> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| format!("连接 SQLite 失败: {}", e))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS favorites (
            platform   TEXT NOT NULL,
            room_id    TEXT NOT NULL,
            title      TEXT NOT NULL,
            username   TEXT NOT NULL,
            view_count INTEGER NOT NULL DEFAULT 0,
            status     TEXT NOT NULL,
            cover      TEXT,
            started_at INTEGER,
            added_at   INTEGER NOT NULL,
            PRIMARY KEY (platform, room_id)
        );
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("创建 favorites 表失败: {}", e))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS history (
            platform   TEXT NOT NULL,
            room_id    TEXT NOT NULL,
            title      TEXT NOT NULL,
            username   TEXT NOT NULL,
            view_count INTEGER NOT NULL DEFAULT 0,
            status     TEXT NOT NULL,
            cover      TEXT,
            started_at INTEGER,
            opened_at  INTEGER NOT NULL,
            PRIMARY KEY (platform, room_id)
        );
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("创建 history 表失败: {}", e))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("创建 app_settings 表失败: {}", e))?;

    POOL.set(pool).map_err(|_| "数据库已初始化".to_string())?;
    Ok(())
}

async fn pool() -> Result<&'static SqlitePool, String> {
    POOL.get().ok_or_else(|| "数据库尚未初始化".to_string())
}

/// 收藏条目（与前端 FavoriteItem 对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteRecord {
    pub platform: String,
    pub id: String,
    pub title: String,
    pub username: String,
    pub view_count: u64,
    pub status: String,
    pub cover: Option<String>,
    pub started_at: Option<i64>,
    pub added_at: i64,
}

/// 观看历史条目（与前端 HistoryItem 对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub platform: String,
    pub id: String,
    pub title: String,
    pub username: String,
    pub view_count: u64,
    pub status: String,
    pub cover: Option<String>,
    pub started_at: Option<i64>,
    pub opened_at: i64,
}

#[tauri::command]
pub async fn load_favorites() -> Result<Vec<FavoriteRecord>, String> {
    let rows = sqlx::query(
        "SELECT platform, room_id, title, username, view_count, status, cover, started_at, added_at \
         FROM favorites ORDER BY added_at DESC",
    )
    .fetch_all(pool().await?)
    .await
    .map_err(|e| format!("读取收藏失败: {}", e))?;

    Ok(rows
        .iter()
        .map(|r| FavoriteRecord {
            platform: r.get("platform"),
            id: r.get("room_id"),
            title: r.get("title"),
            username: r.get("username"),
            view_count: r.get::<i64, _>("view_count").max(0) as u64,
            status: r.get("status"),
            cover: r.get("cover"),
            started_at: r.get("started_at"),
            added_at: r.get("added_at"),
        })
        .collect())
}

#[tauri::command]
pub async fn save_favorites(items: Vec<FavoriteRecord>) -> Result<(), String> {
    let mut tx = pool().await?.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM favorites")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空收藏失败: {}", e))?;
    for item in items {
        sqlx::query(
            "INSERT OR REPLACE INTO favorites \
             (platform, room_id, title, username, view_count, status, cover, started_at, added_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.platform)
        .bind(&item.id)
        .bind(&item.title)
        .bind(&item.username)
        .bind(item.view_count as i64)
        .bind(&item.status)
        .bind(&item.cover)
        .bind(item.started_at)
        .bind(item.added_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入收藏失败: {}", e))?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_history() -> Result<Vec<HistoryRecord>, String> {
    let rows = sqlx::query(
        "SELECT platform, room_id, title, username, view_count, status, cover, started_at, opened_at \
         FROM history ORDER BY opened_at DESC",
    )
    .fetch_all(pool().await?)
    .await
    .map_err(|e| format!("读取历史失败: {}", e))?;

    Ok(rows
        .iter()
        .map(|r| HistoryRecord {
            platform: r.get("platform"),
            id: r.get("room_id"),
            title: r.get("title"),
            username: r.get("username"),
            view_count: r.get::<i64, _>("view_count").max(0) as u64,
            status: r.get("status"),
            cover: r.get("cover"),
            started_at: r.get("started_at"),
            opened_at: r.get("opened_at"),
        })
        .collect())
}

#[tauri::command]
pub async fn save_history(items: Vec<HistoryRecord>) -> Result<(), String> {
    let mut tx = pool().await?.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM history")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清空历史失败: {}", e))?;
    for item in items {
        sqlx::query(
            "INSERT OR REPLACE INTO history \
             (platform, room_id, title, username, view_count, status, cover, started_at, opened_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.platform)
        .bind(&item.id)
        .bind(&item.title)
        .bind(&item.username)
        .bind(item.view_count as i64)
        .bind(&item.status)
        .bind(&item.cover)
        .bind(item.started_at)
        .bind(item.opened_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("写入历史失败: {}", e))?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_setting(key: String) -> Result<Option<String>, String> {
    let row = sqlx::query("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool().await?)
        .await
        .map_err(|e| format!("读取设置失败: {}", e))?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

#[tauri::command]
pub async fn set_setting(key: String, value: String) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool().await?)
    .await
    .map_err(|e| format!("写入设置失败: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_roundtrip() {
        let path = std::env::temp_dir().join(format!("streamflow-db-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        init(path.to_string_lossy().as_ref()).await.expect("初始化失败");

        set_setting("theme".into(), "dark".into())
            .await
            .expect("写入设置失败");
        assert_eq!(
            get_setting("theme".into()).await.expect("读取设置失败"),
            Some("dark".to_string())
        );

        let item = FavoriteRecord {
            platform: "huya".into(),
            id: "988".into(),
            title: "测试直播间".into(),
            username: "tester".into(),
            view_count: 123,
            status: "live".into(),
            cover: None,
            started_at: None,
            added_at: 1,
        };
        save_favorites(vec![item]).await.expect("写入收藏失败");
        let loaded = load_favorites().await.expect("读取收藏失败");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "988");
        assert_eq!(loaded[0].view_count, 123);
    }
}

