import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import type { AddResult, RepoView } from "./api";
import "./App.css";

const NO_GROUP = "Sin grupo";

function App() {
  const [repos, setRepos] = useState<RepoView[]>([]);
  const [filter, setFilter] = useState<string | null>(null);
  const [newGroup, setNewGroup] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setRepos(await api.listRepos());
    } catch (e) {
      setNotice(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

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

  async function add(paths: string[]) {
    if (paths.length === 0) return;
    setBusy(true);
    try {
      const result = await api.addRepos(paths, newGroup.trim() || null);
      setNotice(describe(result));
      await refresh();
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickRepos() {
    const picked = await open({ directory: true, multiple: true, title: "Elige uno o varios repositorios" });
    if (picked) await add(Array.isArray(picked) ? picked : [picked]);
  }

  async function scanFolder() {
    const root = await open({ directory: true, title: "Elige la carpeta que contiene tus repositorios" });
    if (typeof root !== "string") return;
    try {
      const found = await api.findRepos(root);
      if (found.length === 0) setNotice("No encontré repositorios git en esa carpeta.");
      else await add(found);
    } catch (e) {
      setNotice(String(e));
    }
  }

  async function remove(repo: RepoView) {
    await api.removeRepo(repo.path);
    await refresh();
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
          <button disabled={busy} onClick={pickRepos}>Añadir repositorios</button>
          <button disabled={busy} onClick={scanFolder}>Buscar en una carpeta</button>
          <button className="secondary" disabled={busy} onClick={refresh}>Actualizar</button>
        </div>
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
          Aún no hay repositorios. Añádelos uno a uno o elige una carpeta para buscar todos los que contiene.
        </p>
      ) : (
        grouped.map(([group, items]) => (
          <section key={group}>
            <h2>{group} <span className="count">{items.length}</span></h2>
            <ul className="repo-list">
              {items.map((repo) => (
                <RepoRow key={repo.path} repo={repo} onRemove={remove} onGroup={changeGroup} />
              ))}
            </ul>
          </section>
        ))
      )}
    </main>
  );
}

function RepoRow({
  repo,
  onRemove,
  onGroup,
}: {
  repo: RepoView;
  onRemove: (r: RepoView) => void;
  onGroup: (r: RepoView, g: string) => void;
}) {
  const [group, setGroupText] = useState(repo.group ?? "");
  useEffect(() => setGroupText(repo.group ?? ""), [repo.group]);

  return (
    <li className={repo.status.isRepo ? "repo" : "repo invalid"}>
      <div className="repo-info">
        <strong>{repo.name}</strong>
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
