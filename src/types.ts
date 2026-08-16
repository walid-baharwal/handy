export type TargetKind = "project" | "command" | "group";

export interface TargetRef {
  kind: TargetKind;
  id: string;
}

export interface Project {
  id: string;
  name: string;
  baseDir: string;
  commandIds: string[];
}

export interface HandyCommand {
  id: string;
  projectId: string;
  name: string;
  command: string;
  cwd: string;
  stopCommand?: string;
  statusCommand?: string;
}

export interface Group {
  id: string;
  name: string;
  targets: TargetRef[];
}

export interface Config {
  schemaVersion: 1;
  projects: Record<string, Project>;
  commands: Record<string, HandyCommand>;
  groups: Record<string, Group>;
}

export type ProcessStatus =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "completed"
  | "failed";

export interface RuntimeEntry {
  commandId: string;
  status: ProcessStatus;
  managed: boolean;
  exitCode?: number;
  startedAt?: number;
}

export interface LogEntry {
  sequence: number;
  timestamp: number;
  commandId: string;
  stream: "stdout" | "stderr" | "system";
  text: string;
}

export interface Snapshot {
  config: Config;
  runtime: RuntimeEntry[];
  logs: LogEntry[];
  startupWarning?: string;
}

export interface CommandSuggestion {
  name: string;
  command: string;
  cwd: string;
  source: string;
  stopCommand?: string;
  statusCommand?: string;
}
