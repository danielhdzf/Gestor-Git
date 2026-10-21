use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Estado de una carpeta registrada, leído de su carpeta `.git`.
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
///
/// Lee `.git` directamente en vez de lanzar `git`: así añadir o listar repos no crea
/// procesos, algo que algunos antivirus vigilan en programas sin firmar.
pub fn inspect(path: &Path) -> RepoStatus {
    let fail = |msg: &str| RepoStatus {
        is_repo: false,
        branch: None,
        error: Some(msg.into()),
    };
    if !path.is_dir() {
        return fail("La carpeta no existe");
    }
    let Some(git_dir) = git_dir(path) else {
        return fail("No es un repositorio git");
    };
    match fs::read_to_string(git_dir.join("HEAD")) {
        Ok(head) => RepoStatus {
            is_repo: true,
            branch: branch_from_head(&head),
            error: None,
        },
        Err(_) => fail("La carpeta .git está incompleta o dañada"),
    }
}

/// Carpeta con los datos del repo. En worktrees y submódulos `.git` es un archivo
/// con una línea `gitdir: <ruta>` que apunta a ella.
fn git_dir(path: &Path) -> Option<PathBuf> {
    let dot_git = path.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    let target = Path::new(target);
    let dir = if target.is_absolute() { target.to_path_buf() } else { path.join(target) };
    dir.is_dir().then_some(dir)
}

/// `ref: refs/heads/main` da `main`; un HEAD desacoplado (un hash) da `HEAD (abc1234)`.
fn branch_from_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        return Some(reference.strip_prefix("refs/heads/").unwrap_or(reference).to_string());
    }
    let short: String = head.chars().take(7).collect();
    (!short.is_empty()).then(|| format!("HEAD ({short})"))
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
    fn head_desacoplado() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "x"]);
        git(dir.path(), &["checkout", "--detach"]);
        let branch = inspect(dir.path()).branch.unwrap();
        assert!(branch.starts_with("HEAD (") && branch.len() == "HEAD (1234567)".len(), "{branch}");
    }

    #[test]
    fn worktree_con_archivo_git() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("principal");
        let wt = dir.path().join("otra");
        fs::create_dir(&main).unwrap();
        git(&main, &["init", "-b", "main"]);
        git(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "x"]);
        git(&main, &["worktree", "add", "-b", "rama-wt", wt.to_str().unwrap()]);
        assert!(wt.join(".git").is_file());
        let status = inspect(&wt);
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("rama-wt"));
    }

    #[test]
    fn subcarpeta_de_un_repo_no_cuenta() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init"]);
        fs::create_dir(dir.path().join("src")).unwrap();
        assert!(!inspect(&dir.path().join("src")).is_repo);
    }

    #[test]
    fn carpeta_inexistente() {
        let status = inspect(Path::new("/no/existe/de/verdad"));
        assert!(!status.is_repo);
    }
}
