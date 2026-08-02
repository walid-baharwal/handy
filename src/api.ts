import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CommandSuggestion,
  Config,
  Group,
  HandyCommand,
  LogEntry,
  Project,
  RuntimeEntry,
  Snapshot,
  TargetRef,
} from "./types";

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  detect: (baseDir: string) => invoke<CommandSuggestion[]>("detect_commands", { baseDir }),
  saveProject: (project: Project, commands: HandyCommand[]) =>
    invoke<Config>("save_project", { project, commands }),
  deleteProject: (id: string) => invoke<Config>("delete_project", { id }),
  saveGroup: (group: Group) => invoke<Config>("save_group", { group }),
  deleteGroup: (id: string) => invoke<Config>("delete_group", { id }),
  start: (target: TargetRef) => invoke<void>("start_target", { target }),
  stop: (target: TargetRef) => invoke<void>("stop_target", { target }),
  clearLogs: () => invoke<void>("clear_logs"),
  onRuntime: (handler: (entries: RuntimeEntry[]) => void): Promise<UnlistenFn> =>
    listen<RuntimeEntry[]>("runtime-changed", (event) => handler(event.payload)),
  onLogs: (handler: (entries: LogEntry[]) => void): Promise<UnlistenFn> =>
    listen<LogEntry[]>("log-batch", (event) => handler(event.payload)),
};
