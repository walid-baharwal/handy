import assert from "node:assert/strict";
import test from "node:test";
import { appendBoundedLogs } from "../src/lib/runtime.ts";
import type { LogEntry } from "../src/types.ts";

function log(sequence: number, text: string): LogEntry {
  return { sequence, timestamp: 0, commandId: "test", stream: "stdout", text };
}

test("bounded logs keep the newest complete UTF-8 entries", () => {
  let logs: LogEntry[] = [];
  let bytes = 0;

  [logs, bytes] = appendBoundedLogs(logs, bytes, [log(1, "1234"), log(2, "éé")], 8);
  [logs, bytes] = appendBoundedLogs(logs, bytes, [log(3, "last")], 8);

  assert.deepEqual(
    logs.map((entry) => entry.sequence),
    [2, 3],
  );
  assert.equal(bytes, 8);
});
