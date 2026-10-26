import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { AddResult, RepoView } from "./api";
import { RunPanel, RunResults, useRun } from "./Run";
import "./App.css";

const NO_GROUP = "Sin grupo";

function App() {
  const [repos, setRepos] = useState<RepoView[]>([]);
  const [filter, setFilter] = useState<string | null>(null);
  const [newGroup, setNewGroup] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [typedPath, setTypedPath] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const { run, start, cancel, clear } = useRun();
  const running = !!run && !run.done;

  /** Vuelve a leer los repos (y su rama actual). En segundo plano no muestra errores. */
  const refresh = useCallback(async (quiet = false) => {
    try {
      const next = await api.listRepos();
      // Evita repintar la lista cuando nada ha cambiado, que es lo normal en los refrescos periódicos.
      setRepos((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
    } catch (e) {
      if (!quiet) setNotice(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // La rama puede cambiar fuera de la app (un checkout en la terminal, el IDE...). Leer .git/HEAD
  // es barato, así que se relee cada pocos segundos mientras la ventana está visible y al volver a ella.
  useEffect(() => {
    const tick = () => document.visibilityState === "visible" && refresh(true);
    const timer = window.setInterval(tick, 5000);
    window.addEventListener("focus", tick);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", tick);
    };
  }, [refresh]);

  // Al terminar una ejecución (un checkout, un pull...) la rama puede haber cambiado.
  useEffect(() => {
    if (run?.done) refresh(true);
  }, [run?.done, refresh]);

  // Olvida la selección de repos que ya no están o han dejado de ser válidos.
  useEffect(() => {
    const valid = new Set(repos.filter((r) => r.status.isRepo).map((r) => r.path));
    setSelected((sel) => {
      const next = new Set([...sel].filter((p) => valid.has(p)));
      return next.size === sel.size ? sel : next;
    });
  }, [repos]);

  function toggle(paths: string[], on: boolean) {
    setSelected((sel) => {
      const next = new Set(sel);
      for (const p of paths) on ? next.add(p) : next.delete(p);
      return next;
    });
  }

  async function runCommands(label: string, commands: string[], mode: api.RunMode) {
    const targets = repos.filter((r) => selected.has(r.path));
    try {
      await start(targets, label, commands, mode);
    } catch (e) {
      setNotice(String(e));
    }
  }

  const groups = useMemo(
    () => [...new Set(repos.map((r) => r.group).filter((g): g is string => !!g))].sort(),
    [repos],
  );

  const grouped = useMemo(() => {
    const visible = filter === null ? repos : repos.filter((r) => (r.group ?? NO_GROUP) === filter);
    const map = new Map<string, RepoView[]>();
    for (const repo of visible) {
      const key = repo.group ?? NO_GROUP;
      map.set(key, [...(map.get(key) ?? []), repo]);
    }
    return [...map.entries()].sort(([a], [b]) =>
      a === NO_GROUP ? 1 : b === NO_GROUP ? -1 : a.localeCompare(b),
    );
  }, [repos, filter]);

  /** Si `path` es un repo lo añade; si no, añade los repos de sus subcarpetas. */
  async function addPathOrScan(path: string): Promise<boolean> {
    setBusy(true);
    try {
      const group = newGroup.trim() || null;
      const result = await api.addRepos([path], group);
      const notRepo = result.added.length === 0 && result.skipped[0]?.reason === "No es un repositorio git";
      const found = notRepo ? await api.findRepos(path) : [];
      if (found.length > 0) setNotice(describe(await api.addRepos(found, group)));
      else if (notRepo) setNotice("Esa carpeta no es un repositorio git ni contiene ninguno.");
      else setNotice(describe(result));
      await refresh();
      return result.added.length > 0 || found.length > 0;
    } catch (e) {
      setNotice(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }

  // Selector de una sola carpeta: con selección múltiple, CrowdStrike Falcon cierra la app.
  async function pickFolder() {
    const picked = await open({ directory: true, title: "Elige un repositorio o una carpeta que contenga varios" });
    if (typeof picked === "string") await addPathOrScan(picked);
  }

  async function addTypedPath(e: React.FormEvent) {
    e.preventDefault();
    const path = typedPath.trim().replace(/^"(.*)"$/, "$1");
    if (path && (await addPathOrScan(path))) setTypedPath("");
  }

  async function remove(repo: RepoView) {
    await api.removeRepo(repo.path);
    await refresh();
  }

  async function rename(repo: RepoView, name: string) {
    if (name.trim() === repo.name) return;
    try {
      await api.renameRepo(repo.path, name);
      await refresh();
    } catch (e) {
      setNotice(String(e));
    }
  }

  async function changeGroup(repo: RepoView, group: string) {
    const value = group.trim() || null;
    if (value === repo.group) return;
    await api.setGroup(repo.path, value);
    await refresh();
  }

  return (
    <main className="app">
      <header className="toolbar">
        <h1>Gestor Git</h1>
        <div className="actions">
          <input
            list="groups"
            placeholder="Grupo para los nuevos (opcional)"
            value={newGroup}
            onChange={(e) => setNewGroup(e.currentTarget.value)}
          />
          <button disabled={busy} onClick={pickFolder} title="Un repositorio o una carpeta con varios">
            Buscar en una carpeta
          </button>
          <button className="secondary" disabled={busy} onClick={() => refresh()}>Actualizar</button>
        </div>
        <form className="path-row" onSubmit={addTypedPath}>
          <input
            placeholder="O pega una ruta: C:\dev\mi-repo, o una carpeta con varios repos"
            value={typedPath}
            onChange={(e) => setTypedPath(e.currentTarget.value)}
            spellCheck={false}
          />
          <button type="submit" className="secondary" disabled={busy || !typedPath.trim()}>Añadir ruta</button>
        </form>
        <datalist id="groups">
          {groups.map((g) => (
            <option key={g} value={g} />
          ))}
        </datalist>
      </header>

      {notice && (
        <div className="notice">
          <pre>{notice}</pre>
          <button className="secondary" onClick={() => setNotice(null)}>Cerrar</button>
        </div>
      )}

      <RunPanel
        selectedCount={selected.size}
        running={running}
        onRun={runCommands}
        onCancel={cancel}
        onError={setNotice}
      />

      {run && <RunResults run={run} onClear={clear} />}

      {groups.length > 0 && (
        <nav className="filters">
          <Chip active={filter === null} onClick={() => setFilter(null)}>Todos ({repos.length})</Chip>
          {[...groups, NO_GROUP].map((g) => (
            <Chip key={g} active={filter === g} onClick={() => setFilter(g)}>{g}</Chip>
          ))}
        </nav>
      )}

      {repos.length === 0 ? (
        <p className="empty">
          Aún no hay repositorios. Elige un repositorio, o una carpeta y se añadirán todos los que contiene.
        </p>
      ) : (
        grouped.map(([group, items]) => {
          const selectable = items.filter((r) => r.status.isRepo).map((r) => r.path);
          const all = selectable.length > 0 && selectable.every((p) => selected.has(p));
          return (
            <section key={group}>
              <h2>
                <label className="select-group" title="Seleccionar todo el grupo">
                  <input
                    type="checkbox"
                    checked={all}
                    disabled={selectable.length === 0}
                    onChange={() => toggle(selectable, !all)}
                  />
                  {group}
                </label>{" "}
                <span className="count">{items.length}</span>
              </h2>
              <ul className="repo-list">
                {items.map((repo) => (
                  <RepoRow
                    key={repo.path}
                    repo={repo}
                    selected={selected.has(repo.path)}
                    onToggle={(on) => toggle([repo.path], on)}
                    onRemove={remove}
                    onGroup={changeGroup}
                    onRename={rename}
                  />
                ))}
              </ul>
            </section>
          );
        })
      )}
    </main>
  );
}

function RepoRow({
  repo,
  selected,
  onToggle,
  onRemove,
  onGroup,
  onRename,
}: {
  repo: RepoView;
  selected: boolean;
  onToggle: (on: boolean) => void;
  onRemove: (r: RepoView) => void;
  onGroup: (r: RepoView, g: string) => void;
  onRename: (r: RepoView, name: string) => void;
}) {
  const [group, setGroupText] = useState(repo.group ?? "");
  useEffect(() => setGroupText(repo.group ?? ""), [repo.group]);
  const [nameDraft, setNameDraft] = useState<string | null>(null);
  // Enter o Escape cierran la edición; el blur que llega después no debe volver a guardar.
  const renameDone = useRef(false);

  function startRename() {
    renameDone.current = false;
    setNameDraft(repo.name);
  }

  function finishRename(save: boolean) {
    if (renameDone.current) return;
    renameDone.current = true;
    if (nameDraft !== null && save) onRename(repo, nameDraft);
    setNameDraft(null);
  }

  return (
    <li className={repo.status.isRepo ? "repo" : "repo invalid"}>
      <input
        type="checkbox"
        checked={selected}
        disabled={!repo.status.isRepo}
        onChange={(e) => onToggle(e.currentTarget.checked)}
        aria-label={`Seleccionar ${repo.name}`}
      />
      <div className="repo-info">
        {nameDraft === null ? (
          <div className="name-row">
            <strong onDoubleClick={startRename} title="Doble clic para cambiar el nombre">
              {repo.name}
            </strong>
            <button
              className="secondary small icon rename"
              aria-label={`Cambiar el nombre de ${repo.name}`}
              title="Cambiar el nombre (vacío = nombre de la carpeta)"
              onClick={startRename}
            >
              ✎
            </button>
          </div>
        ) : (
          <input
            className="name-input"
            autoFocus
            value={nameDraft}
            placeholder="Vacío = nombre de la carpeta"
            onChange={(e) => setNameDraft(e.currentTarget.value)}
            onBlur={() => finishRename(true)}
            onKeyDown={(e) => {
              if (e.key === "Enter") finishRename(true);
              if (e.key === "Escape") finishRename(false);
            }}
          />
        )}
        <span className="path" title={repo.path}>{repo.path}</span>
      </div>
      {repo.status.isRepo ? (
        <span className="badge">{repo.status.branch ?? "sin commits"}</span>
      ) : (
        <span className="badge error" title={repo.status.error ?? ""}>{repo.status.error}</span>
      )}
      <input
        className="group-input"
        list="groups"
        placeholder="Grupo"
        value={group}
        onChange={(e) => setGroupText(e.currentTarget.value)}
        onBlur={() => onGroup(repo, group)}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <button className="secondary" onClick={() => onRemove(repo)} title="Quitar de la lista (no borra la carpeta)">
        Quitar
      </button>
    </li>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button className={active ? "chip active" : "chip"} onClick={onClick}>
      {children}
    </button>
  );
}

function describe({ added, skipped }: AddResult): string {
  const lines = [`Añadidos: ${added.length}`];
  for (const s of skipped) lines.push(`Omitido ${s.path}: ${s.reason}`);
  return lines.join("\n");
}

export default App;
