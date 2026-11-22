//! Desktop shell: the web interface plus the backend running in the same process.
//!
//! The interface calls the `api` command, which dispatches straight into the backend router
//! (see `invoice_rust::api::dispatch`). There is no listening port: users sign in like anywhere
//! else, and the interface passes its session token with every call.

mod secret;

use invoice_rust::{
    api::{dispatch, router, AppState},
    service::Service,
    store::Store,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

struct Backend {
    router: axum::Router,
}

#[derive(Serialize)]
struct ApiResponse {
    status: u16,
    body: String,
}

#[tauri::command]
async fn api(
    backend: State<'_, Backend>,
    method: String,
    path: String,
    body: Option<String>,
    token: Option<String>,
) -> Result<ApiResponse, String> {
    let (status, body) = dispatch(
        backend.router.clone(),
        token.as_deref(),
        &method,
        &path,
        body,
    )
    .await?;
    Ok(ApiResponse { status, body })
}

/// Asks where to save and writes the file. Returns `false` if the user cancels.
#[tauri::command]
async fn save_file(app: tauri::AppHandle, name: String, content: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name(&name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let secret = secret::load_or_create(&data_dir)?;
            let db_path = data_dir.join("invoices.db");
            let store = Store::open(db_path.to_str().ok_or("invalid data directory")?)?;

            let state = Arc::new(AppState::new(Service::new(store, &secret)));

            // Same background retry as the server: every 5 minutes.
            let background = state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    let _ = background.service.retry_pending(None).await;
                }
            });

            app.manage(Backend {
                router: router(state),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api, save_file])
        .run(tauri::generate_context!())
        .expect("error while running the desktop app");
}
