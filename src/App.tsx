import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { NavButton } from "./components/ui";
import { GroupEditor, GroupsView } from "./features/groups";
import { ProjectEditor, ProjectsView } from "./features/projects";
import { RunningView } from "./features/running";
import { appendBoundedLogs, mergeLogs } from "./lib/runtime";
import type {
  Config,
  Group,
  LogEntry,
  ProcessStatus,
  Project,
  RuntimeEntry,
  TargetRef,
} from "./types";

type View = "projects" | "groups" | "running";

const emptyConfig: Config = { schemaVersion: 1, projects: {}, commands: {}, groups: {} };
const liveStatuses: ProcessStatus[] = ["starting", "running", "stopping"];

export default function App() {
  const [view, setView] = useState<View>("projects");
  const [config, setConfig] = useState<Config>(emptyConfig);
  const [runtime, setRuntime] = useState<RuntimeEntry[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [projectEditor, setProjectEditor] = useState<Project | "new" | null>(null);
  const [groupEditor, setGroupEditor] = useState<Group | "new" | null>(null);
  const [selectedLog, setSelectedLog] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const logBytes = useRef(0);

  useEffect(() => {
    let disposed = false;
    let initialized = false;
    let latestRuntime: RuntimeEntry[] | null = null;
    const pendingLogs: LogEntry[] = [];
    const unlisten: Array<() => void> = [];

    function appendLogs(entries: LogEntry[]) {
      setLogs((current) => {
        const [logs, bytes] = appendBoundedLogs(current, logBytes.current, entries);
        logBytes.current = bytes;
        return logs;
      });
    }

    async function initialize() {
      try {
        const stopRuntime = await api.onRuntime((entries) => {
          if (initialized) setRuntime(entries);
          else latestRuntime = entries;
        });
        if (disposed) return stopRuntime();
        unlisten.push(stopRuntime);

        const stopLogs = await api.onLogs((entries) => {
          if (initialized) appendLogs(entries);
          else pendingLogs.push(...entries);
        });
        if (disposed) return stopLogs();
        unlisten.push(stopLogs);

        const snapshot = await api.snapshot();
        if (disposed) return;
        setConfig(snapshot.config);
        setRuntime(latestRuntime ?? snapshot.runtime);
        const [logs, bytes] = appendBoundedLogs([], 0, mergeLogs(snapshot.logs, pendingLogs));
        logBytes.current = bytes;
        initialized = true;
        setLogs(logs);
        if (snapshot.startupWarning) setError(snapshot.startupWarning);
      } catch (error) {
        if (!disposed) showError(error);
      } finally {
        if (!disposed) setLoading(false);
      }
    }

    void initialize();

    return () => {
      disposed = true;
      unlisten.forEach((fn) => fn());
    };
  }, []);

  const runtimeById = useMemo(
    () => Object.fromEntries(runtime.map((entry) => [entry.commandId, entry])),
    [runtime],
  );
  const runningCount = runtime.filter((entry) => liveStatuses.includes(entry.status)).length;

  function showError(value: unknown) {
    setError(value instanceof Error ? value.message : String(value));
  }

  async function run(target: TargetRef) {
    setError("");
    try {
      await api.start(target);
    } catch (cause) {
      showError(cause);
    }
  }

  async function stop(target: TargetRef) {
    setError("");
    try {
      await api.stop(target);
    } catch (cause) {
      showError(cause);
    }
  }

  async function removeProject(project: Project) {
    if (!confirm(`Delete ${project.name} and its commands?`)) return;
    try {
      setConfig(await api.deleteProject(project.id));
    } catch (cause) {
      showError(cause);
    }
  }

  async function removeGroup(group: Group) {
    if (!confirm(`Delete group ${group.name}?`)) return;
    try {
      setConfig(await api.deleteGroup(group.id));
    } catch (cause) {
      showError(cause);
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">H</span>
          <div>
            <strong>handy</strong>
            <small>local service switchboard</small>
          </div>
        </div>
        <nav>
          <NavButton
            active={view === "projects"}
            onClick={() => setView("projects")}
            icon="01"
            label="Stacks"
          />
          <NavButton
            active={view === "groups"}
            onClick={() => setView("groups")}
            icon="02"
            label="Recipes"
          />
          <NavButton
            active={view === "running"}
            onClick={() => setView("running")}
            icon="03"
            label="Console"
            badge={runningCount || undefined}
          />
        </nav>
        <div className="sidebar-foot">
          <span className="status-dot" /> ON DEVICE <b>v0.1</b>
        </div>
      </aside>

      <main>
        {error && (
          <div className="error-banner">
            <span>{error}</span>
            <button aria-label="Dismiss error" onClick={() => setError("")}>
              ×
            </button>
          </div>
        )}
        {loading ? (
          <div className="loading">Loading Handy…</div>
        ) : view === "projects" ? (
          <ProjectsView
            config={config}
            runtime={runtimeById}
            onAdd={() => setProjectEditor("new")}
            onEdit={setProjectEditor}
            onDelete={removeProject}
            onRun={run}
            onStop={stop}
            onLogs={(id) => {
              setSelectedLog(id);
              setView("running");
            }}
          />
        ) : view === "groups" ? (
          <GroupsView
            config={config}
            runtime={runtimeById}
            onAdd={() => setGroupEditor("new")}
            onEdit={setGroupEditor}
            onDelete={removeGroup}
            onRun={run}
            onStop={stop}
          />
        ) : (
          <RunningView
            config={config}
            runtime={runtimeById}
            logs={logs}
            selected={selectedLog}
            onSelect={setSelectedLog}
            onRun={run}
            onStop={stop}
            onClear={() => {
              void api
                .clearLogs()
                .then((boundary) => {
                  setLogs((current) => {
                    const [logs, bytes] = appendBoundedLogs(
                      [],
                      0,
                      current.filter((entry) => entry.sequence >= boundary),
                    );
                    logBytes.current = bytes;
                    return logs;
                  });
                })
                .catch(showError);
            }}
          />
        )}
      </main>

      {projectEditor && (
        <ProjectEditor
          value={projectEditor === "new" ? undefined : projectEditor}
          config={config}
          onClose={() => setProjectEditor(null)}
          onSave={async (project, commands) => {
            setConfig(await api.saveProject(project, commands));
            setProjectEditor(null);
          }}
          onError={showError}
        />
      )}
      {groupEditor && (
        <GroupEditor
          value={groupEditor === "new" ? undefined : groupEditor}
          config={config}
          onClose={() => setGroupEditor(null)}
          onSave={async (group) => {
            setConfig(await api.saveGroup(group));
            setGroupEditor(null);
          }}
          onError={showError}
        />
      )}
    </div>
  );
}
