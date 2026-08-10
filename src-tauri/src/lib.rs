// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri_plugin_sql::{Builder as SqlBuilder, Migration, MigrationKind};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            SqlBuilder::default()
                .add_migrations(
                    "sqlite:jarvis.db",
                    vec![
                        Migration {
                            version: 1,
                            description: "spine_001",
                            sql: include_str!("../migrations/001_spine.sql"),
                            kind: MigrationKind::Up,
                        },
                        Migration {
                            version: 2,
                            description: "habits_002",
                            sql: include_str!("../migrations/002_habits.sql"),
                            kind: MigrationKind::Up,
                        },
                    ],
                )
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}