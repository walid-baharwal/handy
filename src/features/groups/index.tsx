import { Fragment, useState } from "react";
import { Empty, Glyph, Modal, PageHeader, Status } from "../../components/ui";
import {
  aggregateStatus,
  describeTargets,
  isLive,
  resolveGroup,
  type CommonViewProps,
} from "../../lib/runtime";
import type { Config, Group, TargetRef } from "../../types";

export function GroupsView(
  props: CommonViewProps & {
    onAdd: () => void;
    onEdit: (group: Group) => void;
    onDelete: (group: Group) => void;
  },
) {
  const groups = Object.values(props.config.groups);
  return (
    <section className="page">
      <PageHeader
        eyebrow="Control room / recipes"
        title="Repeatable starts."
        description="Compose the services you reach for together into a single, dependable launch."
        action={
          <button className="primary" onClick={props.onAdd}>
            New recipe <Glyph name="launch" />
          </button>
        }
      />
      {groups.length === 0 ? (
        <Empty
          title="Build a launch recipe"
          text="Tie an API, web app, database, and worker together once. Handy keeps shared services alive when another recipe still needs them."
          action={props.onAdd}
        />
      ) : (
        <div className="table-shell">
          <table className="control-table recipe-table">
            <thead>
              <tr>
                <th scope="col">Recipe</th>
                <th scope="col">Includes</th>
                <th scope="col">State</th>
                <th scope="col">Controls</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((group) => {
                const ids = resolveGroup(group.id, props.config);
                const states = ids.map((id) => props.runtime[id]?.status);
                const active = states.some(isLive);
                return (
                  <Fragment key={group.id}>
                    <tr className="table-parent-row recipe-parent-row">
                      <th scope="row">
                        <div className="table-stack">
                          <span className="group-symbol">
                            <Glyph name="recipe" />
                          </span>
                          <span>
                            <small>LAUNCH RECIPE</small>
                            <strong>{group.name}</strong>
                            <code>{describeTargets(group.targets, props.config)}</code>
                          </span>
                        </div>
                      </th>
                      <td>
                        {ids.length} command{ids.length === 1 ? "" : "s"}
                      </td>
                      <td>
                        <Status label={aggregateStatus(states)} />
                      </td>
                      <td className="table-actions">
                        <div className="manage-actions">
                          <button onClick={() => props.onEdit(group)}>Configure</button>
                          <button className="danger-text" onClick={() => props.onDelete(group)}>
                            Remove
                          </button>
                        </div>
                        {active ? (
                          <button
                            className="stop"
                            onClick={() => props.onStop({ kind: "group", id: group.id })}
                          >
                            Stop
                          </button>
                        ) : (
                          <button
                            className="primary compact"
                            disabled={!ids.length}
                            onClick={() => props.onRun({ kind: "group", id: group.id })}
                          >
                            Run
                          </button>
                        )}
                      </td>
                    </tr>
                    {group.targets.map((target) => (
                      <tr className="table-child-row" key={`${target.kind}-${target.id}`}>
                        <td>
                          <span className="tree-branch" aria-hidden="true" />
                        </td>
                        <td>
                          <div className="target-row">
                            <span className="target-kind">{target.kind[0].toUpperCase()}</span>
                            <strong>{targetName(target, props.config)}</strong>
                          </div>
                        </td>
                        <td className="muted">{target.kind}</td>
                        <td />
                      </tr>
                    ))}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function GroupEditor({
  value,
  config,
  onClose,
  onSave,
  onError,
}: {
  value?: Group;
  config: Config;
  onClose: () => void;
  onSave: (group: Group) => Promise<void>;
  onError: (error: unknown) => void;
}) {
  const [name, setName] = useState(value?.name ?? "");
  const [targets, setTargets] = useState<TargetRef[]>(value?.targets ?? []);
  const [search, setSearch] = useState("");
  const groupId = value?.id ?? crypto.randomUUID();
  const choices: Array<{ target: TargetRef; name: string; detail: string }> = [
    ...Object.values(config.projects).map((project) => ({
      target: { kind: "project" as const, id: project.id },
      name: project.name,
      detail: `${project.commandIds.length} project commands`,
    })),
    ...Object.values(config.commands).map((command) => ({
      target: { kind: "command" as const, id: command.id },
      name: command.name,
      detail: command.command,
    })),
    ...Object.values(config.groups)
      .filter((group) => group.id !== groupId)
      .map((group) => ({
        target: { kind: "group" as const, id: group.id },
        name: group.name,
        detail: "Nested group",
      })),
  ].filter((choice) =>
    `${choice.name} ${choice.detail}`.toLowerCase().includes(search.toLowerCase()),
  );

  function toggle(target: TargetRef) {
    const exists = targets.some((item) => sameTarget(item, target));
    setTargets(exists ? targets.filter((item) => !sameTarget(item, target)) : [...targets, target]);
  }

  return (
    <Modal
      title={value ? "Edit group" : "New group"}
      subtitle="One button can run projects, commands, and nested groups."
      onClose={onClose}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void onSave({ id: groupId, name: name.trim(), targets }).catch(onError);
        }}
      >
        <label>
          Group name
          <input
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Full development stack"
          />
        </label>
        <label>
          Add targets
          <input
            className="search-input"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search projects, commands, and groups…"
          />
        </label>
        <div className="target-picker">
          {choices.map((choice) => {
            const checked = targets.some((item) => sameTarget(item, choice.target));
            return (
              <label
                className={checked ? "checked" : ""}
                key={`${choice.target.kind}-${choice.target.id}`}
              >
                <input type="checkbox" checked={checked} onChange={() => toggle(choice.target)} />
                <span className="target-kind">{choice.target.kind[0].toUpperCase()}</span>
                <div>
                  <strong>{choice.name}</strong>
                  <small>{choice.detail}</small>
                </div>
              </label>
            );
          })}
        </div>
        <p className="selection-count">
          {targets.length} target{targets.length === 1 ? "" : "s"} selected
        </p>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" type="submit">
            Save group
          </button>
        </div>
      </form>
    </Modal>
  );
}

function sameTarget(a: TargetRef, b: TargetRef) {
  return a.kind === b.kind && a.id === b.id;
}

function targetName(target: TargetRef, config: Config) {
  return target.kind === "project"
    ? (config.projects[target.id]?.name ?? "Missing project")
    : target.kind === "command"
      ? (config.commands[target.id]?.name ?? "Missing command")
      : (config.groups[target.id]?.name ?? "Missing recipe");
}
