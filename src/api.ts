import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface RepoStatus {
  isRepo: boolean;
  branch: string | null;
  error: string | null;
}

export interface RepoView {
  path: string;
  name: string;
  group: string | null;
  status: RepoStatus;
}

export interface AddResult {
  added: string[];
  skipped: { path: string; reason: string }[];
}

export const listRepos = () => invoke<RepoView[]>("list_repos");

export const addRepos = (paths: string[], group: string | null) =>
  invoke<AddResult>("add_repos", { paths, group });

export const findRepos = (root: string) => invoke<string[]>("find_repos", { root });

export const removeRepo = (path: string) => invoke<void>("remove_repo", { path });

export const setGroup = (path: string, group: string | null) =>
  invoke<void>("set_group", { path, group });

export type Stream = "stdout" | "stderr" | "info";

export type RunMode = { kind: "parallel"; max: number } | { kind: "sequential"; stopOnError: boolean };

export type RunEvent = { runId: number } & (
  | { type: "started"; repo: string }
  | { type: "output"; repo: string; stream: Stream; line: string; replace: boolean }
  | { type: "finished"; repo: string; success: boolean; code: number | null; error: string | null }
  | { type: "skipped"; repo: string; reason: string }
  | { type: "done"; cancelled: boolean }
);

export const startRun = (repos: string[], commands: string[], mode: RunMode) =>
  invoke<number>("start_run", { repos, commands, mode });

export interface Sequence {
  name: string;
  steps: string[];
}

export const listSequences = () => invoke<Sequence[]>("list_sequences");

export const saveSequence = (original: string | null, name: string, steps: string[]) =>
  invoke<void>("save_sequence", { original, name, steps });

export const deleteSequence = (name: string) => invoke<void>("delete_sequence", { name });

export const cancelRun = (runId: number) => invoke<void>("cancel_run", { runId });

export const onRunEvent = (handler: (e: RunEvent) => void) =>
  listen<RunEvent>("run-event", (e) => handler(e.payload));

export const getShortcuts = () => invoke<string[]>("get_shortcuts");

/** Guarda los atajos y devuelve los que quedan (una lista vacía restaura los de serie). */
export const saveShortcuts = (commands: string[]) => invoke<string[]>("save_shortcuts", { commands });
