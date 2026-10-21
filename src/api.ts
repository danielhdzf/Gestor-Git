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

export type RunMode = { kind: "parallel"; max: number } | { kind: "sequential"; stopOnError: boolean };

export type RunEvent = { runId: number } & (
  | { type: "started"; repo: string }
  | { type: "output"; repo: string; stream: "stdout" | "stderr"; line: string; replace: boolean }
  | { type: "finished"; repo: string; success: boolean; code: number | null; error: string | null }
  | { type: "skipped"; repo: string; reason: string }
  | { type: "done"; cancelled: boolean }
);

export const startRun = (repos: string[], command: string, mode: RunMode) =>
  invoke<number>("start_run", { repos, command, mode });

export const cancelRun = (runId: number) => invoke<void>("cancel_run", { runId });

export const onRunEvent = (handler: (e: RunEvent) => void) =>
  listen<RunEvent>("run-event", (e) => handler(e.payload));
