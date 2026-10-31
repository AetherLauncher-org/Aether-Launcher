use aether_runtime::JavaRuntime;

use super::host::run;

pub(crate) async fn list_java_runtimes(
    app: tauri::AppHandle,
    refresh: bool,
) -> Result<Vec<JavaRuntime>, String> {
    run(app, move |host| {
        aether_app::java::list_runtimes(host, refresh)
    })
    .await
}

pub(crate) async fn pick_java_runtime(
    app: tauri::AppHandle,
) -> Result<Option<JavaRuntime>, String> {
    run(app, |host| aether_app::java::pick_runtime(host)).await
}
