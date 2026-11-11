//! 数据库模块 - SeaORM + SQLite
//! 本地存储房源、历史、设置

use sea_orm::DatabaseConnection;

pub mod history;
pub mod rooms;
pub mod settings;

pub async fn init_db(db_path: &str) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let connection_string = format!("sqlite://{}?mode=rwc", db_path);
    sea_orm::Database::connect(&connection_string).await
}
