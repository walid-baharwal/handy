import type { Config, LogEntry, ProcessStatus, RuntimeEntry, TargetRef } from "../types";

export const LOG_LIMIT_BYTES = 32 * 1024 * 1024;
const textEncoder = new TextEncoder();

export interface CommonViewProps {
  config: Config;
  runtime: Record<string, RuntimeEntry>;
  onRun: (target: TargetRef) => void;
  onStop: (target: TargetRef) => void;
}

export function appendBoundedLogs(
  current: LogEntry[],
  currentBytes: number,
  incoming: LogEntry[],
  limit = LOG_LIMIT_BYTES,
): [LogEntry[], number] {
  const logs = [...current, ...incoming];
  let bytes =
    currentBytes +
    incoming.reduce((total, entry) => total + textEncoder.encode(entry.text).length, 0);
  let remove = 0;
  while (bytes > limit && remove < logs.length) {
    bytes -= textEncoder.encode(logs[remove].text).length;
    remove += 1;
  }
  return [remove ? logs.slice(remove) : logs, bytes];
}

const liveStatuses: ProcessStatus[] = ["starting", "running", "stopping"];

export function isLive(status?: ProcessStatus) {
  return Boolean(status && liveStatuses.includes(status));
}

export function canStop(status?: ProcessStatus, stopCommand?: string) {
  return isLive(status) || (status === "completed" && Boolean(stopCommand?.trim()));
}

export function aggregateStatus(statuses: Array<ProcessStatus | undefined>) {
  if (statuses.length === 0 || statuses.every((status) => !status || status === "stopped")) {
    return "Stopped";
  }
  const live = statuses.filter(isLive).length;
  if (live === statuses.length) return "Running";
  if (live > 0) return "Partial";
  if (statuses.some((status) => status === "failed")) return "Failed";
  if (statuses.every((status) => status === "completed")) return "Completed";
  return "Stopped";
}

export function resolveGroup(id: string, config: Config, visiting = new Set<string>()): string[] {
  if (visiting.has(id)) return [];
  visiting.add(id);
  const ids = new Set<string>();
  for (const target of config.groups[id]?.targets ?? []) {
    if (target.kind === "command") ids.add(target.id);
    if (target.kind === "project") {
      config.projects[target.id]?.commandIds.forEach((commandId) => ids.add(commandId));
    }
    if (target.kind === "group") {
      resolveGroup(target.id, config, visiting).forEach((commandId) => ids.add(commandId));
    }
  }
  visiting.delete(id);
  return [...ids];
}

export function describeTargets(targets: TargetRef[], config: Config) {
  if (!targets.length) return "No targets yet";
  return targets
    .map((target) =>
      target.kind === "project"
        ? config.projects[target.id]?.name
        : target.kind === "command"
          ? config.commands[target.id]?.name
          : config.groups[target.id]?.name,
    )
    .filter(Boolean)
    .join(" · ");
}
