import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../api";
import { Empty, Glyph, Modal, PageHeader, Status } from "../../components/ui";
import { aggregateStatus, canStop, type CommonViewProps } from "../../lib/runtime";
import type { CommandSuggestion, Config, HandyCommand, Project } from "../../types";

export function ProjectsView(
  props: CommonViewProps & {
    onAdd: () => void;
    onEdit: (project: Project) => void;
    onDelete: (project: Project) => void;
    onLogs: (id: string) => void;
  },
) {
  const projects = Object.values(props.config.projects);
  return (
    <section className="page">
      <PageHeader
        eyebrow="Control room / local stacks"
        title="What are we running?"
        description="A deliberate home for every command that normally lives across your terminals."
        action={
          <button className="primary" onClick={props.onAdd}>
            Add a stack <Glyph name="launch" />
          </button>
        }
      />
      {projects.length === 0 ? (
        <Empty
          title="Make your first stack"
          text="Pick a project folder. Handy will surface scripts for you to review—then you decide what to run."
          action={props.onAdd}
        />
      ) : (
        <div className="project-grid">
          {projects.map((project, index) => {
            const commands = project.commandIds
              .map((id) => props.config.commands[id])
              .filter(Boolean);
            const state = aggregateStatus(
              commands.map((command) => props.runtime[command.id]?.status),
            );
            const canStopStack = commands.some((command) =>
              canStop(props.runtime[command.id]?.status, command.stopCommand),
            );
            return (
              <article className={`card project-card tone-${index % 4}`} key={project.id}>
                <div className="project-index">{String(index + 1).padStart(2, "0")}</div>
                <div className="card-head">
                  <div className="folder-icon">
                    <Glyph name="folder" />
                  </div>
                  <div className="card-title">
                    <p className="stack-label">LOCAL STACK</p>
                    <h2>{project.name}</h2>
                    <p title={project.baseDir}>{project.baseDir}</p>
                  </div>
                  <Status label={state} />
                </div>
                <div className="service-list">
                  {commands.length === 0 && <p className="muted">No commands configured.</p>}
                  {commands.map((command) => {
                    const status = props.runtime[command.id]?.status ?? "stopped";
                    return (
                      <div className="service-row" key={command.id}>
                        <span className={`service-light ${status}`} />
                        <div>
                          <strong>{command.name}</strong>
                          <code>{command.command}</code>
                        </div>
                        <div className="row-actions">
                          <button title="View logs" onClick={() => props.onLogs(command.id)}>
                            Logs
                          </button>
                          {canStop(status, command.stopCommand) ? (
                            <button
                              className="stop"
                              onClick={() => props.onStop({ kind: "command", id: command.id })}
                            >
                              Stop
                            </button>
                          ) : (
                            <button
                              onClick={() => props.onRun({ kind: "command", id: command.id })}
                            >
                              Run
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
                <footer className="card-foot">
                  <div>
                    <button onClick={() => props.onEdit(project)}>Configure</button>
                    <button className="danger-text" onClick={() => props.onDelete(project)}>
                      Remove
                    </button>
                  </div>
                  {canStopStack ? (
                    <button
                      className="stop"
                      onClick={() => props.onStop({ kind: "project", id: project.id })}
                    >
                      Stop stack <Glyph name="stop" />
                    </button>
                  ) : (
                    <button
                      className="primary compact"
                      disabled={!commands.length}
                      onClick={() => props.onRun({ kind: "project", id: project.id })}
                    >
                      Run stack <Glyph name="launch" />
                    </button>
                  )}
                </footer>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

export function ProjectEditor({
  value,
  config,
  onClose,
  onSave,
  onError,
}: {
  value?: Project;
  config: Config;
  onClose: () => void;
  onSave: (project: Project, commands: HandyCommand[]) => Promise<void>;
  onError: (error: unknown) => void;
}) {
  const [name, setName] = useState(value?.name ?? "");
  const [baseDir, setBaseDir] = useState(value?.baseDir ?? "");
  const [commands, setCommands] = useState<HandyCommand[]>(
    value?.commandIds.map((id) => config.commands[id]).filter(Boolean) ?? [],
  );
  const [suggestions, setSuggestions] = useState<CommandSuggestion[]>([]);
  const [saving, setSaving] = useState(false);
  const projectId = value?.id ?? crypto.randomUUID();

  async function chooseFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose a project folder",
    });
    if (!selected) return;
    setBaseDir(selected);
    if (!name) setName(selected.split(/[\\/]/).filter(Boolean).at(-1) ?? "Project");
    try {
      setSuggestions(await api.detect(selected));
    } catch (error) {
      onError(error);
    }
  }

  function addSuggestion(suggestion: CommandSuggestion) {
    if (commands.some((command) => command.command === suggestion.command)) return;
    setCommands((current) => [
      ...current,
      {
        id: crypto.randomUUID(),
        projectId,
        name: suggestion.name,
        command: suggestion.command,
        cwd: suggestion.cwd,
        stopCommand: suggestion.stopCommand,
      },
    ]);
  }

  function addManual() {
    setCommands((current) => [
      ...current,
      { id: crypto.randomUUID(), projectId, name: "New command", command: "", cwd: "." },
    ]);
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave(
        {
          id: projectId,
          name: name.trim(),
          baseDir,
          commandIds: commands.map((command) => command.id),
        },
        commands,
      );
    } catch (error) {
      onError(error);
      setSaving(false);
    }
  }

  return (
    <Modal
      title={value ? "Edit project" : "Add project"}
      subtitle="Commands run locally inside this folder."
      onClose={onClose}
      wide
    >
      <form onSubmit={submit}>
        <label>
          Project folder
          <div className="folder-field">
            <input
              required
              value={baseDir}
              onChange={(event) => setBaseDir(event.target.value)}
              placeholder="/path/to/project"
            />
            <button type="button" onClick={() => void chooseFolder()}>
              Browse…
            </button>
          </div>
        </label>
        <label>
          Project name
          <input
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="My application"
          />
        </label>
        {suggestions.length > 0 && (
          <div className="suggestions">
            <div>
              <strong>Detected commands</strong>
              <small>Review before adding. Nothing runs automatically.</small>
            </div>
            {suggestions.map((suggestion) => (
              <button
                type="button"
                key={`${suggestion.source}-${suggestion.command}`}
                onClick={() => addSuggestion(suggestion)}
                disabled={commands.some((command) => command.command === suggestion.command)}
              >
                <span>＋</span>
                <div>
                  <strong>{suggestion.name}</strong>
                  <code>{suggestion.command}</code>
                </div>
                <small>{suggestion.source}</small>
              </button>
            ))}
          </div>
        )}
        <div className="form-section-head">
          <div>
            <strong>Commands</strong>
            <small>Each command gets its own status and logs.</small>
          </div>
          <button type="button" onClick={addManual}>
            ＋ Add manually
          </button>
        </div>
        <div className="command-editor-list">
          {commands.map((command, index) => (
            <div className="command-editor" key={command.id}>
              <span className="drag-index">{index + 1}</span>
              <div className="fields">
                <input
                  aria-label="Command name"
                  required
                  value={command.name}
                  onChange={(event) =>
                    setCommands(updateAt(commands, index, { name: event.target.value }))
                  }
                  placeholder="Service name"
                />
                <input
                  aria-label="Shell command"
                  required
                  className="mono-input"
                  value={command.command}
                  onChange={(event) =>
                    setCommands(updateAt(commands, index, { command: event.target.value }))
                  }
                  placeholder="pnpm dev"
                />
                <div className="split-fields">
                  <input
                    aria-label="Working directory"
                    value={command.cwd}
                    onChange={(event) =>
                      setCommands(updateAt(commands, index, { cwd: event.target.value }))
                    }
                    placeholder="Relative directory (.)"
                  />
                  <input
                    aria-label="Optional stop command"
                    value={command.stopCommand ?? ""}
                    onChange={(event) =>
                      setCommands(
                        updateAt(commands, index, { stopCommand: event.target.value || undefined }),
                      )
                    }
                    placeholder="Optional stop command"
                  />
                </div>
              </div>
              <button
                type="button"
                className="icon-danger"
                aria-label={`Remove ${command.name}`}
                onClick={() => setCommands(commands.filter((_, item) => item !== index))}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" disabled={saving} type="submit">
            {saving ? "Saving…" : "Save project"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function updateAt(commands: HandyCommand[], index: number, patch: Partial<HandyCommand>) {
  return commands.map((command, item) => (item === index ? { ...command, ...patch } : command));
}
