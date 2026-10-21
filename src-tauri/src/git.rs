use std::path::Path;
use std::process::Command;

/// Estado de una carpeta registrada, obtenido preguntando a git.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub error: Option<String>,
}

/// Crea un `git` que en Windows no abre una ventana de consola por cada llamada.
pub fn git_command() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Comprueba que `path` es la raíz de un repositorio git y devuelve su rama actual.
pub fn inspect(path: &Path) -> RepoStatus {
    if !path.is_dir() {
        return RepoStatus {
            is_repo: false,
            branch: None,
            error: Some("La carpeta no existe".into()),
        };
    }

    let output = git_command()
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree", "--abbrev-ref", "HEAD"])
        .output();

    match output {
        Err(e) => RepoStatus {
            is_repo: false,
            branch: None,
            error: Some(format!("No se pudo ejecutar git: {e}")),
        },
        Ok(out) if !out.status.success() => {
            // Un repo recién creado sin commits falla en `--abbrev-ref HEAD`, pero sigue siendo un repo.
            if has_git_dir(path) {
                RepoStatus {
                    is_repo: true,
                    branch: None,
                    error: None,
                }
            } else {
                RepoStatus {
                    is_repo: false,
                    branch: None,
                    error: Some("No es un repositorio git".into()),
                }
            }
        }
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let branch = stdout.lines().nth(1).map(str::trim).map(String::from);
            RepoStatus {
                is_repo: true,
                branch,
                error: None,
            }
        }
    }
}

/// `.git` puede ser una carpeta o, en worktrees y submódulos, un archivo.
pub fn has_git_dir(path: &Path) -> bool {
    path.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = git_command()
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} falló");
    }

    #[test]
    fn detecta_repo_con_rama() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "principal"]);
        git(dir.path(), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "x"]);
        let status = inspect(dir.path());
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("principal"));
    }

    #[test]
    fn repo_sin_commits_sigue_siendo_repo() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init"]);
        assert!(inspect(dir.path()).is_repo);
    }

    #[test]
    fn carpeta_normal_no_es_repo() {
        let dir = tempfile::tempdir().unwrap();
        let status = inspect(dir.path());
        assert!(!status.is_repo);
        assert!(status.error.is_some());
    }

    #[test]
    fn carpeta_inexistente() {
        let status = inspect(Path::new("/no/existe/de/verdad"));
        assert!(!status.is_repo);
    }
}
