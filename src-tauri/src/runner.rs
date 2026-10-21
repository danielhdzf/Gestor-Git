use crate::git::git_command;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufReader, Read};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Mode {
    /// Todos los repos a la vez, con como mucho `max` procesos git simultáneos.
    #[serde(rename_all = "camelCase")]
    Parallel { max: usize },
    /// Un repo detrás de otro, en el orden recibido.
    #[serde(rename_all = "camelCase")]
    Sequential { stop_on_error: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunEvent {
    Started { repo: String },
    Output { repo: String, stream: Stream, line: String, replace: bool },
    #[serde(rename_all = "camelCase")]
    Finished { repo: String, success: bool, code: Option<i32>, error: Option<String> },
    /// El repo no llegó a ejecutarse: se canceló o un fallo anterior paró la secuencia.
    Skipped { repo: String, reason: String },
    Done { cancelled: bool },
}

/// Divide un comando como lo escribiría el usuario, respetando comillas simples y dobles.
/// Acepta que empiece por `git`, que se descarta porque siempre se ejecuta git.
pub fn parse_command(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;

    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_arg = true;
            }
            None if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            None => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if quote.is_some() {
        return Err("Hay comillas sin cerrar".into());
    }
    if in_arg {
        args.push(current);
    }
    if args.first().is_some_and(|a| a.eq_ignore_ascii_case("git")) {
        args.remove(0);
    }
    if args.is_empty() {
        return Err("Escribe un comando git, por ejemplo: pull".into());
    }
    Ok(args)
}

/// Ejecuta `git <args>` en cada repo según `mode`, informando de todo por `emit`.
/// Bloquea hasta terminar; `cancel` detiene lo que esté en marcha y salta lo pendiente.
pub fn run<F>(repos: Vec<String>, args: Vec<String>, mode: Mode, cancel: Arc<AtomicBool>, emit: F)
where
    F: Fn(RunEvent) + Send + Sync + 'static,
{
    let emit = Arc::new(emit);
    match mode {
        Mode::Parallel { max } => {
            let queue = Arc::new(Mutex::new(VecDeque::from(repos)));
            let workers = max.max(1).min(queue.lock().unwrap().len().max(1));
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    let (queue, args, cancel, emit) =
                        (queue.clone(), args.clone(), cancel.clone(), emit.clone());
                    thread::spawn(move || {
                        while let Some(repo) = queue.lock().unwrap().pop_front() {
                            if cancel.load(Ordering::SeqCst) {
                                emit(skipped(repo, "Cancelado"));
                            } else {
                                run_one(&repo, &args, &cancel, &*emit);
                            }
                        }
                    })
                })
                .collect();
            for h in handles {
                let _ = h.join();
            }
        }
        Mode::Sequential { stop_on_error } => {
            let mut stopped: Option<&str> = None;
            for repo in repos {
                if cancel.load(Ordering::SeqCst) {
                    stopped = Some("Cancelado");
                }
                if let Some(reason) = stopped {
                    emit(skipped(repo, reason));
                    continue;
                }
                if !run_one(&repo, &args, &cancel, &*emit) && stop_on_error {
                    stopped = Some("Detenido porque falló un repo anterior");
                }
            }
        }
    }
    emit(RunEvent::Done { cancelled: cancel.load(Ordering::SeqCst) });
}

fn skipped(repo: String, reason: &str) -> RunEvent {
    RunEvent::Skipped { repo, reason: reason.into() }
}

/// Devuelve si el comando terminó bien.
fn run_one(repo: &str, args: &[String], cancel: &AtomicBool, emit: &(dyn Fn(RunEvent) + Send + Sync)) -> bool {
    emit(RunEvent::Started { repo: repo.into() });

    let spawned = git_command()
        .arg("-C")
        .arg(repo)
        .args(args)
        // Sin terminal no hay a quién preguntar la contraseña: mejor fallar que quedarse colgado.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            emit(RunEvent::Finished {
                repo: repo.into(),
                success: false,
                code: None,
                error: Some(format!("No se pudo ejecutar git: {e}")),
            });
            return false;
        }
    };

    let result = thread::scope(|s| {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        s.spawn(|| forward_lines(stdout, repo, Stream::Stdout, emit));
        s.spawn(|| forward_lines(stderr, repo, Stream::Stderr, emit));
        wait_or_cancel(&mut child, cancel)
    });

    let event = match result {
        Ok(Some(status)) => RunEvent::Finished {
            repo: repo.into(),
            success: status.success(),
            code: status.code(),
            error: None,
        },
        Ok(None) => RunEvent::Finished {
            repo: repo.into(),
            success: false,
            code: None,
            error: Some("Cancelado".into()),
        },
        Err(e) => RunEvent::Finished {
            repo: repo.into(),
            success: false,
            code: None,
            error: Some(e.to_string()),
        },
    };
    let ok = matches!(event, RunEvent::Finished { success: true, .. });
    emit(event);
    ok
}

/// Espera al proceso; si piden cancelar lo mata y devuelve `None`.
fn wait_or_cancel(child: &mut Child, cancel: &AtomicBool) -> std::io::Result<Option<std::process::ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Reenvía la salida línea a línea. Git redibuja las barras de progreso con `\r`,
/// así que una línea que sigue a un `\r` se marca con `replace` para que sustituya
/// a la anterior en vez de acumular cientos de líneas de porcentaje.
fn forward_lines(pipe: impl Read, repo: &str, stream: Stream, emit: &(dyn Fn(RunEvent) + Send + Sync)) {
    let mut reader = BufReader::new(pipe);
    let mut buf = [0u8; 4096];
    let mut line = Vec::new();
    let mut overwrite_next = false;
    let send = |line: &mut Vec<u8>, replace: bool| {
        emit(RunEvent::Output {
            repo: repo.into(),
            stream,
            line: String::from_utf8_lossy(line).into_owned(),
            replace,
        });
        line.clear();
    };
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            match b {
                b'\r' => {
                    if !line.is_empty() {
                        send(&mut line, overwrite_next);
                        overwrite_next = true;
                    }
                }
                // Un `\r\n` de Windows: la línea ya salió con el `\r`.
                b'\n' if line.is_empty() && overwrite_next => overwrite_next = false,
                b'\n' => {
                    send(&mut line, overwrite_next);
                    overwrite_next = false;
                }
                _ => line.push(b),
            }
        }
    }
    if !line.is_empty() {
        send(&mut line, overwrite_next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parsea_comandos() {
        assert_eq!(parse_command("git pull --rebase").unwrap(), ["pull", "--rebase"]);
        assert_eq!(parse_command("  status  ").unwrap(), ["status"]);
        assert_eq!(
            parse_command(r#"commit -m "un mensaje largo" --author='Yo <a@b>'"#).unwrap(),
            ["commit", "-m", "un mensaje largo", "--author=Yo <a@b>"]
        );
        assert_eq!(parse_command(r#"commit -m """#).unwrap(), ["commit", "-m", ""]);
        assert!(parse_command("git").is_err());
        assert!(parse_command("   ").is_err());
        assert!(parse_command("commit -m \"sin cerrar").is_err());
    }

    fn init_repo(dir: &Path) {
        for args in [
            &["init", "-b", "main"][..],
            &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "x"],
        ] {
            assert!(git_command().arg("-C").arg(dir).args(args).output().unwrap().status.success());
        }
    }

    fn collect(repos: Vec<String>, cmd: &str, mode: Mode) -> Vec<RunEvent> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        run(repos, parse_command(cmd).unwrap(), mode, Arc::new(AtomicBool::new(false)), move |e| {
            sink.lock().unwrap().push(e)
        });
        Arc::try_unwrap(events).unwrap().into_inner().unwrap()
    }

    fn finished(events: &[RunEvent], repo: &str) -> Option<bool> {
        events.iter().find_map(|e| match e {
            RunEvent::Finished { repo: r, success, .. } if r == repo => Some(*success),
            _ => None,
        })
    }

    #[test]
    fn paralelo_ejecuta_todos_y_captura_salida() {
        let dirs: Vec<_> = (0..4).map(|_| tempfile::tempdir().unwrap()).collect();
        let repos: Vec<String> = dirs
            .iter()
            .map(|d| {
                init_repo(d.path());
                d.path().to_string_lossy().into_owned()
            })
            .collect();
        let events = collect(repos.clone(), "git rev-parse --abbrev-ref HEAD", Mode::Parallel { max: 2 });

        for repo in &repos {
            assert_eq!(finished(&events, repo), Some(true));
            assert!(events.contains(&RunEvent::Output {
                repo: repo.clone(),
                stream: Stream::Stdout,
                line: "main".into(),
                replace: false,
            }));
        }
        assert_eq!(events.last(), Some(&RunEvent::Done { cancelled: false }));
    }

    #[test]
    fn secuencial_para_tras_un_fallo() {
        let good = tempfile::tempdir().unwrap();
        init_repo(good.path());
        let bad = tempfile::tempdir().unwrap(); // no es repo: git falla
        let (good, bad) = (
            good.path().to_string_lossy().into_owned(),
            bad.path().to_string_lossy().into_owned(),
        );
        let events = collect(
            vec![good.clone(), bad.clone(), good.clone()],
            "status",
            Mode::Sequential { stop_on_error: true },
        );
        assert_eq!(finished(&events, &bad), Some(false));
        assert!(matches!(&events[events.len() - 2], RunEvent::Skipped { repo, .. } if *repo == good));
        assert!(events.iter().any(|e| matches!(e, RunEvent::Output { repo, stream: Stream::Stderr, .. } if *repo == bad)));
    }

    #[test]
    fn secuencial_sin_parar_sigue() {
        let bad = tempfile::tempdir().unwrap();
        let good = tempfile::tempdir().unwrap();
        init_repo(good.path());
        let (bad, good) = (
            bad.path().to_string_lossy().into_owned(),
            good.path().to_string_lossy().into_owned(),
        );
        let events = collect(vec![bad, good.clone()], "status", Mode::Sequential { stop_on_error: false });
        assert_eq!(finished(&events, &good), Some(true));
    }

    #[test]
    fn cancelado_antes_de_empezar_salta_todo() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        run(
            vec!["a".into(), "b".into()],
            vec!["status".into()],
            Mode::Parallel { max: 4 },
            Arc::new(AtomicBool::new(true)),
            move |e| sink.lock().unwrap().push(e),
        );
        let events = events.lock().unwrap();
        assert_eq!(events.iter().filter(|e| matches!(e, RunEvent::Skipped { .. })).count(), 2);
        assert_eq!(events.last(), Some(&RunEvent::Done { cancelled: true }));
    }

    fn lines_of(input: &[u8]) -> Vec<(String, bool)> {
        let out = Mutex::new(Vec::new());
        forward_lines(input, "r", Stream::Stderr, &|e| {
            if let RunEvent::Output { line, replace, .. } = e {
                out.lock().unwrap().push((line, replace));
            }
        });
        out.into_inner().unwrap()
    }

    #[test]
    fn progreso_con_retorno_de_carro_sustituye_la_linea() {
        let l = |s: &str, r: bool| (s.to_string(), r);
        assert_eq!(
            lines_of(b"inicio\nRecibiendo 10%\rRecibiendo 100%, hecho.\nfin"),
            [l("inicio", false), l("Recibiendo 10%", false), l("Recibiendo 100%, hecho.", true), l("fin", false)]
        );
        assert_eq!(lines_of(b"uno\r\ndos\r\n"), [l("uno", false), l("dos", false)]);
        assert_eq!(lines_of(b"a\n\nb\n"), [l("a", false), l("", false), l("b", false)]);
    }

    #[cfg(unix)]
    #[test]
    fn cancelar_mata_el_proceso_en_marcha() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            flag.store(true, Ordering::SeqCst);
        });
        let start = std::time::Instant::now();
        assert!(wait_or_cancel(&mut child, &cancel).unwrap().is_none());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn modo_desde_json() {
        let m: Mode = serde_json::from_str(r#"{"kind":"sequential","stopOnError":true}"#).unwrap();
        assert!(matches!(m, Mode::Sequential { stop_on_error: true }));
        let m: Mode = serde_json::from_str(r#"{"kind":"parallel","max":3}"#).unwrap();
        assert!(matches!(m, Mode::Parallel { max: 3 }));
    }
}
