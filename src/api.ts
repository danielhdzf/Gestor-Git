import { invoke } from "@tauri-apps/api/core";

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
