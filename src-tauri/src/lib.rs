mod config;
mod git;
mod runner;

use config::{AddError, Config, Repo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    config: Mutex<Config>,
    file: PathBuf,
    /// Ejecuciones en marcha y la señal para cancelarlas.
    runs: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    next_run: AtomicU64,
}

/// Lo que ve la interfaz: el repo guardado más su estado actual en git.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoView {
    #[serde(flatten)]
    repo: Repo,
    status: git::RepoStatus,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddResult {
    added: Vec<String>,
    skipped: Vec<Skipped>,
}

#[derive(serde::Serialize)]
struct Skipped {
    path: String,
    reason: String,
}

fn persist(state: &AppState, cfg: &Config) -> Result<(), String> {
    cfg.save(&state.file)
        .map_err(|e| format!("No se pudo guardar la configuración: {e}"))
}

#[tauri::command]
fn list_repos(state: State<AppState>) -> Vec<RepoView> {
    let repos = state.config.lock().unwrap().repos.clone();
    repos
        .into_iter()
        .map(|repo| {
            let status = git::inspect(Path::new(&repo.path));
            RepoView { repo, status }
        })
        .collect()
}

#[tauri::command]
fn add_repos(
    paths: Vec<String>,
    group: Option<String>,
    state: State<AppState>,
) -> Result<AddResult, String> {
    let mut cfg = state.config.lock().unwrap();
    let mut result = AddResult {
        added: vec![],
        skipped: vec![],
    };
    for path in paths {
        let status = git::inspect(Path::new(&path));
        if !status.is_repo {
            let reason = status.error.unwrap_or_else(|| "No es un repositorio git".into());
            result.skipped.push(Skipped { path, reason });
            continue;
        }
        match cfg.add(Path::new(&path), group.clone()) {
            Ok(repo) => result.added.push(repo.path.clone()),
            Err(AddError::Duplicate) => result.skipped.push(Skipped {
                path,
                reason: "Ya estaba en la lista".into(),
            }),
        }
    }
    persist(&state, &cfg)?;
    Ok(result)
}

/// Busca repositorios en las subcarpetas directas de `root` (por ejemplo `C:\dev`).
#[tauri::command]
fn find_repos(root: String) -> Result<Vec<String>, String> {
    config::find_repos_in(Path::new(&root))
        .map(|found| {
            found
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|e| format!("No se pudo leer la carpeta: {e}"))
}

#[tauri::command]
fn remove_repo(path: String, state: State<AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap();
    if cfg.remove(&path) {
        persist(&state, &cfg)?;
    }
    Ok(())
}

#[tauri::command]
fn set_group(path: String, group: Option<String>, state: State<AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap();
    if cfg.set_group(&path, group) {
        persist(&state, &cfg)?;
    }
    Ok(())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RunEventPayload {
    run_id: u64,
    #[serde(flatten)]
    event: runner::RunEvent,
}

/// Lanza `command` en `repos` en segundo plano. El progreso llega como eventos `run-event`.
#[tauri::command]
fn start_run(
    repos: Vec<String>,
    command: String,
    mode: runner::Mode,
    app: AppHandle,
    state: State<AppState>,
) -> Result<u64, String> {
    if repos.is_empty() {
        return Err("Selecciona al menos un repositorio".into());
    }
    let args = runner::parse_command(&command)?;
    let run_id = state.next_run.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    state.runs.lock().unwrap().insert(run_id, cancel.clone());

    std::thread::spawn(move || {
        let emitter = app.clone();
        runner::run(repos, args, mode, cancel, move |event| {
            let _ = emitter.emit("run-event", RunEventPayload { run_id, event });
        });
        app.state::<AppState>().runs.lock().unwrap().remove(&run_id);
    });
    Ok(run_id)
}

#[tauri::command]
fn cancel_run(run_id: u64, state: State<AppState>) {
    if let Some(cancel) = state.runs.lock().unwrap().get(&run_id) {
        cancel.store(true, Ordering::SeqCst);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let file = app.path().app_config_dir()?.join("config.json");
            let config = Config::load(&file)?;
            app.manage(AppState {
                config: Mutex::new(config),
                file,
                runs: Mutex::new(HashMap::new()),
                next_run: AtomicU64::new(1),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_repos,
            add_repos,
            find_repos,
            remove_repo,
            set_group,
            start_run,
            cancel_run
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar la aplicación");
}
