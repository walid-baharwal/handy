import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type {
  CommandSuggestion,
  Config,
  Group,
  HandyCommand,
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

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    void api
      .snapshot()
      .then((snapshot) => {
        if (disposed) return;
        setConfig(snapshot.config);
        setRuntime(snapshot.runtime);
        setLogs(snapshot.logs);
      })
      .catch(showError)
      .finally(() => setLoading(false));
    void api.onRuntime(setRuntime).then((fn) => (disposed ? fn() : unlisten.push(fn)));
    void api.onLogs((entries) => setLogs((current) => [...current, ...entries])).then((fn) => (disposed ? fn() : unlisten.push(fn)));
    return () => {
      disposed = true;
      unlisten.forEach((fn) => fn());
    };
  }, []);

  const runtimeById = useMemo(() => Object.fromEntries(runtime.map((entry) => [entry.commandId, entry])), [runtime]);
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
          <div><strong>Handy</strong><small>Local services</small></div>
        </div>
        <nav>
          <NavButton active={view === "projects"} onClick={() => setView("projects")} icon="▦" label="Projects" />
          <NavButton active={view === "groups"} onClick={() => setView("groups")} icon="⌘" label="Groups" />
          <NavButton active={view === "running"} onClick={() => setView("running")} icon="▶" label="Running" badge={runningCount || undefined} />
        </nav>
        <div className="sidebar-foot"><span className="status-dot" /> Local only · v0.1</div>
      </aside>

      <main>
        {error && <div className="error-banner"><span>{error}</span><button onClick={() => setError("")}>×</button></div>}
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
            onLogs={(id) => { setSelectedLog(id); setView("running"); }}
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
            onClear={() => { void api.clearLogs(); setLogs([]); }}
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

function NavButton({ active, onClick, icon, label, badge }: { active: boolean; onClick: () => void; icon: string; label: string; badge?: number }) {
  return <button className={active ? "nav-active" : ""} onClick={onClick}><span>{icon}</span>{label}{badge && <b>{badge}</b>}</button>;
}

function PageHeader({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: React.ReactNode }) {
  return <header className="page-header"><div><small>{eyebrow}</small><h1>{title}</h1><p>{description}</p></div>{action}</header>;
}

interface CommonViewProps {
  config: Config;
  runtime: Record<string, RuntimeEntry>;
  onRun: (target: TargetRef) => void;
  onStop: (target: TargetRef) => void;
}

function ProjectsView(props: CommonViewProps & {
  onAdd: () => void;
  onEdit: (project: Project) => void;
  onDelete: (project: Project) => void;
  onLogs: (id: string) => void;
}) {
  const projects = Object.values(props.config.projects);
  return <section className="page">
    <PageHeader eyebrow="Workspace" title="Projects" description="Start each development stack without rebuilding your terminal layout." action={<button className="primary" onClick={props.onAdd}>＋ Add project</button>} />
    {projects.length === 0 ? <Empty icon="▦" title="Add your first project" text="Choose a folder and Handy will find package scripts and Docker Compose files. Nothing runs until you press Run." action={props.onAdd} /> :
      <div className="project-grid">{projects.map((project) => {
        const commands = project.commandIds.map((id) => props.config.commands[id]).filter(Boolean);
        const state = aggregateStatus(commands.map((command) => props.runtime[command.id]?.status));
        const active = commands.some((command) => isLive(props.runtime[command.id]?.status));
        return <article className="card project-card" key={project.id}>
          <div className="card-head"><div className="folder-icon">⌁</div><div className="card-title"><h2>{project.name}</h2><p title={project.baseDir}>{project.baseDir}</p></div><Status label={state} /></div>
          <div className="service-list">
            {commands.length === 0 && <p className="muted">No commands configured.</p>}
            {commands.map((command) => {
              const status = props.runtime[command.id]?.status ?? "stopped";
              return <div className="service-row" key={command.id}>
                <span className={`service-light ${status}`} /><div><strong>{command.name}</strong><code>{command.command}</code></div>
                <div className="row-actions"><button title="View logs" onClick={() => props.onLogs(command.id)}>Logs</button>{isLive(status) ? <button className="stop" onClick={() => props.onStop({ kind: "command", id: command.id })}>Stop</button> : <button onClick={() => props.onRun({ kind: "command", id: command.id })}>Run</button>}</div>
              </div>;
            })}
          </div>
          <footer className="card-foot"><div><button onClick={() => props.onEdit(project)}>Edit</button><button className="danger-text" onClick={() => props.onDelete(project)}>Delete</button></div>{active ? <button className="stop" onClick={() => props.onStop({ kind: "project", id: project.id })}>■ Stop all</button> : <button className="primary compact" disabled={!commands.length} onClick={() => props.onRun({ kind: "project", id: project.id })}>▶ Run all</button>}</footer>
        </article>;
      })}</div>}
  </section>;
}

function GroupsView(props: CommonViewProps & { onAdd: () => void; onEdit: (group: Group) => void; onDelete: (group: Group) => void }) {
  const groups = Object.values(props.config.groups);
  return <section className="page">
    <PageHeader eyebrow="Reusable stacks" title="Groups" description="Combine projects, commands, and other groups. Shared services only stop when nobody else needs them." action={<button className="primary" onClick={props.onAdd}>＋ New group</button>} />
    {groups.length === 0 ? <Empty icon="⌘" title="Create a reusable stack" text="For example, combine your API, web app, database, and worker behind one Run button." action={props.onAdd} /> :
      <div className="group-list">{groups.map((group) => {
        const ids = resolveGroup(group.id, props.config);
        const states = ids.map((id) => props.runtime[id]?.status);
        const active = states.some(isLive);
        return <article className="card group-card" key={group.id}>
          <div className="group-symbol">⌘</div><div className="group-main"><h2>{group.name}</h2><p>{describeTargets(group.targets, props.config)}</p><div className="chips"><span>{ids.length} command{ids.length === 1 ? "" : "s"}</span><Status label={aggregateStatus(states)} /></div></div>
          <div className="group-actions">{active ? <button className="stop" onClick={() => props.onStop({ kind: "group", id: group.id })}>■ Stop</button> : <button className="primary compact" disabled={!ids.length} onClick={() => props.onRun({ kind: "group", id: group.id })}>▶ Run group</button>}<button onClick={() => props.onEdit(group)}>Edit</button><button className="danger-text" onClick={() => props.onDelete(group)}>Delete</button></div>
        </article>;
      })}</div>}
  </section>;
}

function RunningView(props: CommonViewProps & { logs: LogEntry[]; selected: string | null; onSelect: (id: string | null) => void; onClear: () => void }) {
  const [search, setSearch] = useState("");
  const commands = Object.values(props.config.commands).filter((command) => props.runtime[command.id] || props.logs.some((log) => log.commandId === command.id));
  const visibleLogs = props.logs.filter((log) => (!props.selected || log.commandId === props.selected) && (!search || log.text.toLowerCase().includes(search.toLowerCase())));
  return <section className="running-page">
    <PageHeader eyebrow="Current session" title="Running & logs" description="One view for every process Handy started in this session." />
    <div className="running-layout">
      <aside className="process-list"><button className={!props.selected ? "selected" : ""} onClick={() => props.onSelect(null)}><span className="service-light running" /><div><strong>All services</strong><small>{commands.length} visible</small></div></button>{commands.map((command) => {
        const status = props.runtime[command.id]?.status ?? "stopped";
        return <button className={props.selected === command.id ? "selected" : ""} key={command.id} onClick={() => props.onSelect(command.id)}><span className={`service-light ${status}`} /><div><strong>{command.name}</strong><small>{status}</small></div></button>;
      })}</aside>
      <div className="console-panel"><div className="console-toolbar"><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search session logs…" /><span>{visibleLogs.length} lines</span><button onClick={props.onClear}>Clear</button>{props.selected && (isLive(props.runtime[props.selected]?.status) ? <button className="stop" onClick={() => props.onStop({ kind: "command", id: props.selected! })}>Stop</button> : <button onClick={() => props.onRun({ kind: "command", id: props.selected! })}>Run</button>)}</div><div className="console">{visibleLogs.length === 0 ? <div className="console-empty">Logs will appear here when a command runs.</div> : visibleLogs.map((log) => <div className={`log-line ${log.stream}`} key={log.sequence}><time>{new Date(log.timestamp).toLocaleTimeString()}</time>{!props.selected && <b>{props.config.commands[log.commandId]?.name ?? log.commandId}</b>}<span>{log.text}</span></div>)}</div></div>
    </div>
  </section>;
}

function ProjectEditor({ value, config, onClose, onSave, onError }: { value?: Project; config: Config; onClose: () => void; onSave: (project: Project, commands: HandyCommand[]) => Promise<void>; onError: (error: unknown) => void }) {
  const [name, setName] = useState(value?.name ?? "");
  const [baseDir, setBaseDir] = useState(value?.baseDir ?? "");
  const [commands, setCommands] = useState<HandyCommand[]>(value?.commandIds.map((id) => config.commands[id]).filter(Boolean) ?? []);
  const [suggestions, setSuggestions] = useState<CommandSuggestion[]>([]);
  const [saving, setSaving] = useState(false);
  const projectId = value?.id ?? crypto.randomUUID();

  async function chooseFolder() {
    const selected = await open({ directory: true, multiple: false, title: "Choose a project folder" });
    if (!selected) return;
    setBaseDir(selected);
    if (!name) setName(selected.split(/[\\/]/).filter(Boolean).at(-1) ?? "Project");
    try { setSuggestions(await api.detect(selected)); } catch (error) { onError(error); }
  }

  function addSuggestion(suggestion: CommandSuggestion) {
    if (commands.some((command) => command.command === suggestion.command)) return;
    setCommands((current) => [...current, { id: crypto.randomUUID(), projectId, name: suggestion.name, command: suggestion.command, cwd: suggestion.cwd }]);
  }

  function addManual() {
    setCommands((current) => [...current, { id: crypto.randomUUID(), projectId, name: "New command", command: "", cwd: "." }]);
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave({ id: projectId, name: name.trim(), baseDir, commandIds: commands.map((command) => command.id) }, commands);
    } catch (error) {
      onError(error);
      setSaving(false);
    }
  }

  return <Modal title={value ? "Edit project" : "Add project"} subtitle="Commands run locally inside this folder." onClose={onClose} wide>
    <form onSubmit={submit}>
      <label>Project folder<div className="folder-field"><input required value={baseDir} onChange={(event) => setBaseDir(event.target.value)} placeholder="/path/to/project" /><button type="button" onClick={() => void chooseFolder()}>Browse…</button></div></label>
      <label>Project name<input required value={name} onChange={(event) => setName(event.target.value)} placeholder="My application" /></label>
      {suggestions.length > 0 && <div className="suggestions"><div><strong>Detected commands</strong><small>Review before adding. Nothing runs automatically.</small></div>{suggestions.map((suggestion) => <button type="button" key={`${suggestion.source}-${suggestion.command}`} onClick={() => addSuggestion(suggestion)} disabled={commands.some((command) => command.command === suggestion.command)}><span>＋</span><div><strong>{suggestion.name}</strong><code>{suggestion.command}</code></div><small>{suggestion.source}</small></button>)}</div>}
      <div className="form-section-head"><div><strong>Commands</strong><small>Each command gets its own status and logs.</small></div><button type="button" onClick={addManual}>＋ Add manually</button></div>
      <div className="command-editor-list">{commands.map((command, index) => <div className="command-editor" key={command.id}><span className="drag-index">{index + 1}</span><div className="fields"><input aria-label="Command name" required value={command.name} onChange={(event) => setCommands(updateAt(commands, index, { name: event.target.value }))} placeholder="Service name" /><input aria-label="Shell command" required className="mono-input" value={command.command} onChange={(event) => setCommands(updateAt(commands, index, { command: event.target.value }))} placeholder="pnpm dev" /><div className="split-fields"><input aria-label="Working directory" value={command.cwd} onChange={(event) => setCommands(updateAt(commands, index, { cwd: event.target.value }))} placeholder="Relative directory (.)" /><input aria-label="Optional stop command" value={command.stopCommand ?? ""} onChange={(event) => setCommands(updateAt(commands, index, { stopCommand: event.target.value || undefined }))} placeholder="Optional stop command" /></div></div><button type="button" className="icon-danger" onClick={() => setCommands(commands.filter((_, item) => item !== index))}>×</button></div>)}</div>
      <div className="modal-actions"><button type="button" onClick={onClose}>Cancel</button><button className="primary" disabled={saving} type="submit">{saving ? "Saving…" : "Save project"}</button></div>
    </form>
  </Modal>;
}

function GroupEditor({ value, config, onClose, onSave, onError }: { value?: Group; config: Config; onClose: () => void; onSave: (group: Group) => Promise<void>; onError: (error: unknown) => void }) {
  const [name, setName] = useState(value?.name ?? "");
  const [targets, setTargets] = useState<TargetRef[]>(value?.targets ?? []);
  const [search, setSearch] = useState("");
  const groupId = value?.id ?? crypto.randomUUID();
  const choices: Array<{ target: TargetRef; name: string; detail: string }> = [
    ...Object.values(config.projects).map((project) => ({ target: { kind: "project" as const, id: project.id }, name: project.name, detail: `${project.commandIds.length} project commands` })),
    ...Object.values(config.commands).map((command) => ({ target: { kind: "command" as const, id: command.id }, name: command.name, detail: command.command })),
    ...Object.values(config.groups).filter((group) => group.id !== groupId).map((group) => ({ target: { kind: "group" as const, id: group.id }, name: group.name, detail: "Nested group" })),
  ].filter((choice) => `${choice.name} ${choice.detail}`.toLowerCase().includes(search.toLowerCase()));

  function toggle(target: TargetRef) {
    const exists = targets.some((item) => sameTarget(item, target));
    setTargets(exists ? targets.filter((item) => !sameTarget(item, target)) : [...targets, target]);
  }

  return <Modal title={value ? "Edit group" : "New group"} subtitle="One button can run projects, commands, and nested groups." onClose={onClose}>
    <form onSubmit={(event) => { event.preventDefault(); void onSave({ id: groupId, name: name.trim(), targets }).catch(onError); }}>
      <label>Group name<input required value={name} onChange={(event) => setName(event.target.value)} placeholder="Full development stack" /></label>
      <label>Add targets<input className="search-input" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search projects, commands, and groups…" /></label>
      <div className="target-picker">{choices.map((choice) => {
        const checked = targets.some((item) => sameTarget(item, choice.target));
        return <label className={checked ? "checked" : ""} key={`${choice.target.kind}-${choice.target.id}`}><input type="checkbox" checked={checked} onChange={() => toggle(choice.target)} /><span className="target-kind">{choice.target.kind[0].toUpperCase()}</span><div><strong>{choice.name}</strong><small>{choice.detail}</small></div></label>;
      })}</div>
      <p className="selection-count">{targets.length} target{targets.length === 1 ? "" : "s"} selected</p>
      <div className="modal-actions"><button type="button" onClick={onClose}>Cancel</button><button className="primary" type="submit">Save group</button></div>
    </form>
  </Modal>;
}

function Modal({ title, subtitle, onClose, wide, children }: { title: string; subtitle: string; onClose: () => void; wide?: boolean; children: React.ReactNode }) {
  return <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><div className={`modal ${wide ? "modal-wide" : ""}`} role="dialog" aria-modal="true"><header><div><h2>{title}</h2><p>{subtitle}</p></div><button onClick={onClose}>×</button></header>{children}</div></div>;
}

function Empty({ icon, title, text, action }: { icon: string; title: string; text: string; action: () => void }) {
  return <div className="empty-state"><span>{icon}</span><h2>{title}</h2><p>{text}</p><button className="primary" onClick={action}>Get started</button></div>;
}

function Status({ label }: { label: string }) {
  return <span className={`status-pill ${label.toLowerCase().replace(" ", "-")}`}><i />{label}</span>;
}

function updateAt(commands: HandyCommand[], index: number, patch: Partial<HandyCommand>) {
  return commands.map((command, item) => item === index ? { ...command, ...patch } : command);
}

function sameTarget(a: TargetRef, b: TargetRef) {
  return a.kind === b.kind && a.id === b.id;
}

function isLive(status?: ProcessStatus) {
  return Boolean(status && liveStatuses.includes(status));
}

function aggregateStatus(statuses: Array<ProcessStatus | undefined>) {
  if (statuses.length === 0 || statuses.every((status) => !status || status === "stopped")) return "Stopped";
  const live = statuses.filter(isLive).length;
  if (live === statuses.length) return "Running";
  if (live > 0) return "Partial";
  if (statuses.some((status) => status === "failed")) return "Failed";
  if (statuses.every((status) => status === "completed")) return "Completed";
  return "Stopped";
}

function resolveGroup(id: string, config: Config, visiting = new Set<string>()): string[] {
  if (visiting.has(id)) return [];
  visiting.add(id);
  const ids = new Set<string>();
  for (const target of config.groups[id]?.targets ?? []) {
    if (target.kind === "command") ids.add(target.id);
    if (target.kind === "project") config.projects[target.id]?.commandIds.forEach((commandId) => ids.add(commandId));
    if (target.kind === "group") resolveGroup(target.id, config, visiting).forEach((commandId) => ids.add(commandId));
  }
  visiting.delete(id);
  return [...ids];
}

function describeTargets(targets: TargetRef[], config: Config) {
  if (!targets.length) return "No targets yet";
  return targets.map((target) => target.kind === "project" ? config.projects[target.id]?.name : target.kind === "command" ? config.commands[target.id]?.name : config.groups[target.id]?.name).filter(Boolean).join(" · ");
}
