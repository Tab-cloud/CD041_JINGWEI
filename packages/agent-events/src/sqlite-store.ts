import { Database } from "bun:sqlite"
import type { DurableEvent } from "@jingwei/agent-protocol"
import type { DurableEventStore, EventDraft } from "./store.ts"

const EVENT_SCHEMA_VERSION = 1

export class EventSchemaMigrationError extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options)
    this.name = "EventSchemaMigrationError"
  }
}

export class SqliteEventStore implements DurableEventStore, Disposable {
  readonly database: Database

  constructor(path = ":memory:") {
    this.database = new Database(path, { create: true, strict: true })
    try {
      this.database.exec("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
      migrate(this.database)
    } catch (error) {
      this.database.close()
      throw error
    }
  }

  append<T>(draft: EventDraft<T>): DurableEvent<T> {
    return this.database.transaction(() => {
      const row = this.database
        .query<{ sequence: number }, [string]>(
          "SELECT COALESCE(MAX(sequence), 0) AS sequence FROM runtime_events WHERE aggregate_id = ?",
        )
        .get(draft.aggregateId)
      const event: DurableEvent<T> = {
        schemaVersion: "0.1",
        aggregateId: draft.aggregateId,
        sequence: (row?.sequence ?? 0) + 1,
        timestamp: draft.timestamp ?? new Date().toISOString(),
        ...(draft.sessionId ? { sessionId: draft.sessionId } : {}),
        ...(draft.runId ? { runId: draft.runId } : {}),
        ...(draft.taskId ? { taskId: draft.taskId } : {}),
        ...(draft.traceId ? { traceId: draft.traceId } : {}),
        type: draft.type,
        data: draft.data,
      }
      this.database
        .query(
          `INSERT INTO runtime_events
            (aggregate_id, sequence, schema_version, timestamp, session_id, run_id, task_id, trace_id, type, data_json)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        )
        .run(
          event.aggregateId,
          event.sequence,
          event.schemaVersion,
          event.timestamp,
          event.sessionId ?? null,
          event.runId ?? null,
          event.taskId ?? null,
          event.traceId ?? null,
          event.type,
          JSON.stringify(event.data),
        )
      return event
    })()
  }

  readAfter(aggregateId: string, sequence = 0, limit = 1_000): DurableEvent[] {
    const rows = this.database
      .query<EventRow, [string, number, number]>(
        `SELECT aggregate_id, sequence, schema_version, timestamp, session_id, run_id, task_id, trace_id, type, data_json
         FROM runtime_events WHERE aggregate_id = ? AND sequence > ? ORDER BY sequence ASC LIMIT ?`,
      )
      .all(aggregateId, sequence, limit)
    return rows.map(toEvent)
  }

  latestSequence(aggregateId: string): number {
    return (
      this.database
        .query<{ sequence: number }, [string]>(
          "SELECT COALESCE(MAX(sequence), 0) AS sequence FROM runtime_events WHERE aggregate_id = ?",
        )
        .get(aggregateId)?.sequence ?? 0
    )
  }

  close(): void {
    this.database.close()
  }

  [Symbol.dispose](): void {
    this.close()
  }
}

interface EventRow {
  aggregate_id: string
  sequence: number
  schema_version: string
  timestamp: string
  session_id: string | null
  run_id: string | null
  task_id: string | null
  trace_id: string | null
  type: string
  data_json: string
}

function toEvent(row: EventRow): DurableEvent {
  if (row.schema_version !== "0.1") {
    throw new EventSchemaMigrationError(`Unsupported durable event schema: ${row.schema_version}`)
  }
  return {
    schemaVersion: "0.1",
    aggregateId: row.aggregate_id,
    sequence: row.sequence,
    timestamp: row.timestamp,
    ...(row.session_id ? { sessionId: row.session_id } : {}),
    ...(row.run_id ? { runId: row.run_id } : {}),
    ...(row.task_id ? { taskId: row.task_id } : {}),
    ...(row.trace_id ? { traceId: row.trace_id } : {}),
    type: row.type,
    data: JSON.parse(row.data_json),
  }
}

function migrate(database: Database): void {
  try {
    database.exec(
      `CREATE TABLE IF NOT EXISTS runtime_schema_migrations (
        component TEXT PRIMARY KEY,
        version INTEGER NOT NULL,
        applied_at TEXT NOT NULL
      );`,
    )
    const current = database
      .query<{ version: number }, [string]>(
        "SELECT version FROM runtime_schema_migrations WHERE component = ?",
      )
      .get("agent-events")?.version
    if (current !== undefined && current > EVENT_SCHEMA_VERSION) {
      throw new Error(`database version ${current} is newer than runtime version ${EVENT_SCHEMA_VERSION}`)
    }
    if ((current ?? 0) < 1) {
      database.transaction(() => {
        database.exec(
          `CREATE TABLE runtime_events (
            aggregate_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK(sequence > 0),
            schema_version TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            session_id TEXT,
            run_id TEXT,
            task_id TEXT,
            trace_id TEXT,
            type TEXT NOT NULL,
            data_json TEXT NOT NULL,
            PRIMARY KEY (aggregate_id, sequence)
          );
          CREATE INDEX runtime_events_session_idx ON runtime_events(session_id, sequence);`,
        )
        database
          .query(
            `INSERT INTO runtime_schema_migrations(component, version, applied_at)
             VALUES (?, ?, ?)
             ON CONFLICT(component) DO UPDATE SET version = excluded.version, applied_at = excluded.applied_at`,
          )
          .run("agent-events", 1, new Date().toISOString())
      })()
    }
  } catch (error) {
    throw new EventSchemaMigrationError("Durable event migration failed; runtime writes are disabled", {
      cause: error,
    })
  }
}
