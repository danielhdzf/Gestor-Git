use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Un repositorio registrado. La ruta hace de identificador.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
}

/// Una lista de comandos git con nombre, que se ejecuta en orden en cada repo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<Repo>,
    #[serde(default)]
    pub sequences: Vec<Sequence>,
}

#[derive(Debug, PartialEq)]
pub enum AddError {
    Duplicate,
}

impl Config {
    /// Carga la configuración; si el archivo no existe empieza vacía.
    pub fn load(file: &Path) -> io::Result<Self> {
        match fs::read_to_string(file) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Escribe en un archivo temporal y lo renombra, para no dejar la config a medias.
    pub fn save(&self, file: &Path) -> io::Result<()> {
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, file)
    }

    pub fn add(&mut self, path: &Path, group: Option<String>) -> Result<&Repo, AddError> {
        let key = normalize(path);
        if self.repos.iter().any(|r| same_path(&r.path, &key)) {
            return Err(AddError::Duplicate);
        }
        let name = Path::new(&key)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.clone());
        self.repos.push(Repo {
            path: key,
            name,
            group: clean_group(group),
        });
        self.repos.sort_by_key(|r| r.name.to_lowercase());
        Ok(self.repos.iter().find(|r| same_path(&r.path, &normalize(path))).unwrap())
    }

    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.repos.len();
        self.repos.retain(|r| !same_path(&r.path, path));
        self.repos.len() != before
    }

    pub fn set_group(&mut self, path: &str, group: Option<String>) -> bool {
        match self.repos.iter_mut().find(|r| same_path(&r.path, path)) {
            Some(repo) => {
                repo.group = clean_group(group);
                true
            }
            None => false,
        }
    }
}

impl Config {
    /// Crea o reemplaza una secuencia. `original` es su nombre anterior si se está editando.
    pub fn save_sequence(&mut self, original: Option<&str>, name: &str, steps: Vec<String>) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("La secuencia necesita un nombre".into());
        }
        let steps: Vec<String> = steps
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if steps.is_empty() {
            return Err("Añade al menos un comando".into());
        }
        for (i, step) in steps.iter().enumerate() {
            crate::runner::parse_command(step).map_err(|e| format!("Línea {}: {e}", i + 1))?;
        }
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if self
            .sequences
            .iter()
            .any(|s| same(&s.name, name) && !original.is_some_and(|o| same(o, &s.name)))
        {
            return Err(format!("Ya existe una secuencia llamada \"{name}\""));
        }
        if let Some(o) = original {
            self.sequences.retain(|s| !same(&s.name, o));
        }
        self.sequences.push(Sequence { name: name.into(), steps });
        self.sequences.sort_by_key(|s| s.name.to_lowercase());
        Ok(())
    }

    pub fn delete_sequence(&mut self, name: &str) -> bool {
        let before = self.sequences.len();
        self.sequences.retain(|s| s.name != name);
        self.sequences.len() != before
    }
}

/// Lista las subcarpetas directas de `root` que son repositorios git.
pub fn find_repos_in(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && crate::git::has_git_dir(p))
        .collect();
    found.sort();
    Ok(found)
}

fn clean_group(group: Option<String>) -> Option<String> {
    group.map(|g| g.trim().to_string()).filter(|g| !g.is_empty())
}

fn normalize(path: &Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() { s.into_owned() } else { trimmed.to_string() }
}

/// En Windows las rutas no distinguen mayúsculas ni el tipo de barra.
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"))
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_y_carga() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("config.json");
        let mut cfg = Config::default();
        cfg.add(Path::new("/a/proyecto"), Some("trabajo".into())).unwrap();
        cfg.save(&file).unwrap();
        let loaded = Config::load(&file).unwrap();
        assert_eq!(loaded.repos, cfg.repos);
        assert_eq!(loaded.repos[0].name, "proyecto");
    }

    #[test]
    fn sin_archivo_empieza_vacia() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Config::load(&dir.path().join("nada.json")).unwrap().repos.is_empty());
    }

    #[test]
    fn rechaza_duplicados_aunque_lleven_barra_final() {
        let mut cfg = Config::default();
        cfg.add(Path::new("/a/b"), None).unwrap();
        assert_eq!(cfg.add(Path::new("/a/b/"), None).unwrap_err(), AddError::Duplicate);
    }

    #[test]
    fn quita_y_agrupa() {
        let mut cfg = Config::default();
        cfg.add(Path::new("/a/b"), None).unwrap();
        assert!(cfg.set_group("/a/b", Some("  front ".into())));
        assert_eq!(cfg.repos[0].group.as_deref(), Some("front"));
        assert!(cfg.set_group("/a/b", Some("".into())));
        assert_eq!(cfg.repos[0].group, None);
        assert!(cfg.remove("/a/b"));
        assert!(!cfg.remove("/a/b"));
    }

    #[test]
    fn secuencias() {
        let mut cfg = Config::default();
        let steps = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        cfg.save_sequence(None, " Actualizar ", steps(&["fetch", "  ", "git pull"])).unwrap();
        assert_eq!(cfg.sequences[0], Sequence { name: "Actualizar".into(), steps: steps(&["fetch", "git pull"]) });

        assert!(cfg.save_sequence(None, "actualizar", steps(&["status"])).is_err(), "nombre repetido");
        assert!(cfg.save_sequence(None, "", steps(&["status"])).is_err());
        assert!(cfg.save_sequence(None, "Vacía", steps(&["", " "])).is_err());
        let err = cfg.save_sequence(None, "Mala", steps(&["status", "commit -m \"x"])).unwrap_err();
        assert!(err.starts_with("Línea 2"));

        // Renombrar al editar no choca consigo misma.
        cfg.save_sequence(Some("Actualizar"), "ACTUALIZAR", steps(&["pull"])).unwrap();
        assert_eq!(cfg.sequences.len(), 1);
        assert_eq!(cfg.sequences[0].name, "ACTUALIZAR");

        assert!(cfg.delete_sequence("ACTUALIZAR"));
        assert!(cfg.sequences.is_empty());
    }

    #[test]
    fn config_antigua_sin_secuencias_carga() {
        let cfg: Config = serde_json::from_str(r#"{"repos":[]}"#).unwrap();
        assert!(cfg.sequences.is_empty());
    }

    #[test]
    fn encuentra_repos_en_subcarpetas() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("uno/.git")).unwrap();
        fs::create_dir_all(dir.path().join("dos")).unwrap();
        fs::create_dir_all(dir.path().join("tres/.git")).unwrap();
        let found = find_repos_in(dir.path()).unwrap();
        let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
        assert_eq!(names, ["tres", "uno"]);
    }
}
