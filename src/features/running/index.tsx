import { useLayoutEffect, useRef, useState } from "react";
import { PageHeader } from "../../components/ui";
import { canStop, isLive, type CommonViewProps } from "../../lib/runtime";
import type { LogEntry } from "../../types";

export function RunningView(
  props: CommonViewProps & {
    logs: LogEntry[];
    selected: string | null;
    onSelect: (id: string | null) => void;
    onClear: () => void;
  },
) {
  const [search, setSearch] = useState("");
  const consoleRef = useRef<HTMLDivElement>(null);
  const followLogs = useRef(true);
  const commands = Object.values(props.config.commands).filter(
    (command) =>
      props.runtime[command.id] || props.logs.some((log) => log.commandId === command.id),
  );
  const visibleLogs = props.logs.filter(
    (log) =>
      (!props.selected || log.commandId === props.selected) &&
      (!search || log.text.toLowerCase().includes(search.toLowerCase())),
  );
  const lastVisibleSequence = visibleLogs.at(-1)?.sequence;
  const selectedEntry = props.selected ? props.runtime[props.selected] : undefined;
  const selectedCommand = props.selected ? props.config.commands[props.selected] : undefined;
  const selectedCanStop = canStop(
    selectedEntry?.status,
    selectedCommand?.stopCommand,
    selectedEntry?.managed,
  );

  useLayoutEffect(() => {
    followLogs.current = true;
    const console = consoleRef.current;
    if (console) console.scrollTop = console.scrollHeight;
  }, [props.selected, search]);

  useLayoutEffect(() => {
    const console = consoleRef.current;
    if (console && followLogs.current) console.scrollTop = console.scrollHeight;
  }, [lastVisibleSequence]);

  return (
    <section className="running-page">
      <PageHeader
        eyebrow="Control room / live output"
        title="The machine is talking."
        description="Everything Handy starts in this session, collected without asking you to hunt through terminal tabs."
      />
      <div className="running-layout">
        <aside className="process-list">
          <button
            className={!props.selected ? "selected" : ""}
            onClick={() => props.onSelect(null)}
          >
            <span className="service-light running" />
            <div>
              <strong>All services</strong>
              <small>{commands.length} visible</small>
            </div>
          </button>
          {commands.map((command) => {
            const status = props.runtime[command.id]?.status ?? "stopped";
            return (
              <button
                className={props.selected === command.id ? "selected" : ""}
                key={command.id}
                onClick={() => props.onSelect(command.id)}
              >
                <span className={`service-light ${status}`} />
                <div>
                  <strong>{command.name}</strong>
                  <small>{status}</small>
                </div>
              </button>
            );
          })}
        </aside>
        <div className="console-panel">
          <div className="console-toolbar">
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search session logs…"
            />
            <span>{visibleLogs.length} lines</span>
            <button onClick={props.onClear}>Clear</button>
            {props.selected &&
              (selectedCanStop ? (
                <button
                  className="stop"
                  onClick={() => props.onStop({ kind: "command", id: props.selected! })}
                >
                  Stop
                </button>
              ) : isLive(selectedEntry?.status) ? (
                <button disabled>Running externally</button>
              ) : (
                <button onClick={() => props.onRun({ kind: "command", id: props.selected! })}>
                  Run
                </button>
              ))}
          </div>
          <div
            className="console"
            ref={consoleRef}
            onScroll={(event) => {
              const console = event.currentTarget;
              followLogs.current =
                console.scrollHeight - console.scrollTop - console.clientHeight <= 24;
            }}
          >
            {visibleLogs.length === 0 ? (
              <div className="console-empty">Logs will appear here when a command runs.</div>
            ) : (
              visibleLogs.map((log) => (
                <div
                  className={`log-line ${log.stream}${props.selected ? " selected-log" : ""}`}
                  key={log.sequence}
                >
                  <time>{new Date(log.timestamp).toLocaleTimeString()}</time>
                  {!props.selected && (
                    <b>{props.config.commands[log.commandId]?.name ?? log.commandId}</b>
                  )}
                  <span>{log.text}</span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
