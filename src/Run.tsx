import { useEffect, useRef, useState } from "react";
import * as api from "./api";
import type { RunEvent, RunMode } from "./api";

export type RepoRunState = "pending" | "running" | "ok" | "error" | "skipped";

export interface RepoResult {
  path: string;
  name: string;
  state: RepoRunState;
  lines: { stream: "stdout" | "stderr"; text: string }[];
  note: string | null;
}

export interface Run {
  id: number;
  command: string;
  results: RepoResult[];
  done: boolean;
  cancelled: boolean;
}

// Evita que un comando muy ruidoso deje la interfaz sin memoria.
const MAX_LINES = 2000;

/** Guarda el estado de la ejecución actual y aplica los eventos del backend en bloque, una vez por frame. */
export function useRun() {
  const [run, setRun] = useState<Run | null>(null);
  const pending = useRef<RunEvent[]>([]);
  const frame = useRef<number | null>(null);

  useEffect(() => {
    const unlisten = api.onRunEvent((event) => {
      pending.current.push(event);
      if (frame.current === null) {
        frame.current = requestAnimationFrame(() => {
          frame.current = null;
          const batch = pending.current;
          pending.current = [];
          setRun((current) => (current ? applyEvents(current, batch) : current));
        });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function start(repos: { path: string; name: string }[], command: string, mode: RunMode) {
    const id = await api.startRun(repos.map((r) => r.path), command, mode);
    setRun({
      id,
      command,
      done: false,
      cancelled: false,
      results: repos.map((r) => ({ ...r, state: "pending", lines: [], note: null })),
    });
  }

  async function cancel() {
    if (run && !run.done) await api.cancelRun(run.id);
  }

  return { run, start, cancel, clear: () => setRun(null) };
}

function applyEvents(run: Run, events: RunEvent[]): Run {
  const mine = events.filter((e) => e.runId === run.id);
  if (mine.length === 0) return run;

  // Copia solo los repos que reciben eventos; los demás conservan su objeto y no se vuelven a pintar.
  const byRepo = new Map(run.results.map((r) => [r.path, r]));
  const touched = new Set<string>();
  const edit = (path: string) => {
    const r = byRepo.get(path);
    if (!r) return undefined;
    if (!touched.has(path)) {
      touched.add(path);
      byRepo.set(path, { ...r, lines: [...r.lines] });
    }
    return byRepo.get(path)!;
  };
  let { done, cancelled } = run;
  for (const e of mine) {
    if (e.type === "done") {
      done = true;
      cancelled = e.cancelled;
      continue;
    }
    const r = edit(e.repo);
    if (!r) continue;
    switch (e.type) {
      case "started":
        r.state = "running";
        break;
      case "output":
        if (e.replace && r.lines.length > 0) r.lines[r.lines.length - 1] = { stream: e.stream, text: e.line };
        else if (r.lines.length < MAX_LINES) r.lines.push({ stream: e.stream, text: e.line });
        break;
      case "finished":
        r.state = e.success ? "ok" : "error";
        r.note = e.error ?? (e.success ? null : `Código de salida ${e.code ?? "?"}`);
        break;
      case "skipped":
        r.state = "skipped";
        r.note = e.reason;
        break;
    }
  }
  return { ...run, done, cancelled, results: run.results.map((r) => byRepo.get(r.path)!) };
}

export function RunPanel({
  selectedCount,
  running,
  onRun,
  onCancel,
}: {
  selectedCount: number;
  running: boolean;
  onRun: (command: string, mode: RunMode) => void;
  onCancel: () => void;
}) {
  const [command, setCommand] = useState("status -sb");
  const [kind, setKind] = useState<RunMode["kind"]>("parallel");
  const [max, setMax] = useState(4);
  const [stopOnError, setStopOnError] = useState(true);

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (running || !command.trim() || selectedCount === 0) return;
    onRun(command, kind === "parallel" ? { kind, max } : { kind, stopOnError });
  }

  return (
    <form className="run-panel" onSubmit={submit}>
      <div className="command-row">
        <span className="prompt">git</span>
        <input
          className="command"
          value={command}
          onChange={(e) => setCommand(e.currentTarget.value)}
          placeholder="pull --rebase"
          spellCheck={false}
        />
        {running ? (
          <button type="button" className="danger" onClick={onCancel}>Cancelar</button>
        ) : (
          <button type="submit" disabled={selectedCount === 0 || !command.trim()}>
            Ejecutar en {selectedCount} {selectedCount === 1 ? "repo" : "repos"}
          </button>
        )}
      </div>
      <div className="options">
        <label>
          <input type="radio" checked={kind === "parallel"} onChange={() => setKind("parallel")} /> En paralelo
        </label>
        {kind === "parallel" && (
          <label>
            máx.
            <input
              type="number"
              min={1}
              max={32}
              value={max}
              onChange={(e) => setMax(Math.max(1, Number(e.currentTarget.value) || 1))}
            />
            a la vez
          </label>
        )}
        <label>
          <input type="radio" checked={kind === "sequential"} onChange={() => setKind("sequential")} /> Uno tras otro
        </label>
        {kind === "sequential" && (
          <label>
            <input type="checkbox" checked={stopOnError} onChange={(e) => setStopOnError(e.currentTarget.checked)} />
            Parar si uno falla
          </label>
        )}
      </div>
    </form>
  );
}

const STATE_LABEL: Record<RepoRunState, string> = {
  pending: "En espera",
  running: "Ejecutando…",
  ok: "Correcto",
  error: "Error",
  skipped: "Omitido",
};

export function RunResults({ run, onClear }: { run: Run; onClear: () => void }) {
  const counts = run.results.reduce(
    (acc, r) => ({ ...acc, [r.state]: (acc[r.state] ?? 0) + 1 }),
    {} as Partial<Record<RepoRunState, number>>,
  );
  const summary = (["ok", "error", "skipped", "running", "pending"] as const)
    .filter((s) => counts[s])
    .map((s) => `${STATE_LABEL[s]}: ${counts[s]}`)
    .join(" · ");

  return (
    <section className="results">
      <div className="results-header">
        <h2>
          git {run.command} <span className="count">{run.done ? (run.cancelled ? "cancelado" : "terminado") : "en curso"}</span>
        </h2>
        <span className="summary">{summary}</span>
        {run.done && <button className="secondary" onClick={onClear}>Cerrar</button>}
      </div>
      <ul className="result-list">
        {run.results.map((r) => (
          <ResultRow key={r.path} result={r} />
        ))}
      </ul>
    </section>
  );
}

function ResultRow({ result }: { result: RepoResult }) {
  const [open, setOpen] = useState(false);
  const opened = useRef(false);
  // Los errores se abren solos la primera vez para que se vean sin buscar.
  useEffect(() => {
    if (result.state === "error" && !opened.current) {
      opened.current = true;
      setOpen(true);
    }
  }, [result.state]);

  const hasOutput = result.lines.length > 0;
  return (
    <li className={`result ${result.state}`}>
      <button className="result-head" onClick={() => setOpen(!open)} disabled={!hasOutput}>
        <span className={`dot ${result.state}`} />
        <strong>{result.name}</strong>
        <span className="state">{STATE_LABEL[result.state]}</span>
        {result.note && <span className="note">{result.note}</span>}
        {hasOutput && <span className="toggle">{open ? "▾" : "▸"}</span>}
      </button>
      {open && hasOutput && (
        <pre className="output">
          {result.lines.map((l, i) => (
            <div key={i} className={l.stream}>{l.text}</div>
          ))}
          {result.lines.length >= MAX_LINES && <div className="stderr">… salida recortada</div>}
        </pre>
      )}
    </li>
  );
}
