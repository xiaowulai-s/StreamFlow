//! 系统托盘和通知
//! 提供后台任务状态控制

use tauri::{CustomMenuItem, Menu, Submenu};
use tauri::TrayIcon;

pub fn create_menu() -> Menu {
    let quit = CustomMenuItem::new("quit".to_string(), "退出");
    let reload = CustomMenuItem::new("reload".to_string(), "刷新");
    let settings = CustomMenuItem::new("settings".to_string(), "设置");
    
    let context_menu = Submenu::new("系统托盘", Menu::new().add_item(settings).add_item(reload).add_item(quit));
    Menu::new().add_submenu(context_menu)
}

pub fn create_tray() -> TrayIcon {
    TrayIcon::new()
        .with_menu(create_menu())
        .with_tooltip("StreamFlow 直播聚合")
        .show()
}
