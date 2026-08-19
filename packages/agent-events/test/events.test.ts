import { Database } from "bun:sqlite"
import { afterEach, describe, expect, test } from "bun:test"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import {
  DurableEventBus,
  EventSchemaMigrationError,
  MemoryEventStore,
  rebuildProjection,
  SqliteEventStore,
} from "../src/index.ts"

const tempDirectories: string[] = []

afterEach(() => {
  for (const directory of tempDirectories.splice(0)) {
    removeTemporaryDirectory(directory)
  }
})

describe("durable events", () => {
  test("assigns aggregate-local sequence and rebuilds projections", () => {
    const store = new MemoryEventStore()
    store.append({ aggregateId: "a", type: "added", data: { value: 2 } })
    store.append({ aggregateId: "b", type: "added", data: { value: 9 } })
    store.append({ aggregateId: "a", type: "added", data: { value: 3 } })

    expect(store.readAfter("a").map((event) => event.sequence)).toEqual([1, 2])
    expect(
      rebuildProjection(
        0,
        store.readAfter("a"),
        (state, event) => state + (event.data as { value: number }).value,
      ),
    ).toBe(5)
  })

  test("bridges history and live stream without loss or duplication", async () => {
    const bus = new DurableEventBus(new MemoryEventStore(), 5)
    bus.append({ aggregateId: "session", type: "one", data: {} })
    const controller = new AbortController()
    const received: number[] = []
    const consume = (async () => {
      for await (const event of bus.events("session", 0, controller.signal)) {
        received.push(event.sequence)
        if (received.length === 3) controller.abort()
      }
    })()
    await Bun.sleep(5)
    bus.append({ aggregateId: "session", type: "two", data: {} })
    bus.append({ aggregateId: "session", type: "three", data: {} })
    await consume
    expect(received).toEqual([1, 2, 3])
  })

  test("persists events in SQLite across process-like reopen", () => {
    const path = tempDatabase()
    const first = new SqliteEventStore(path)
    first.append({ aggregateId: "session", type: "input.admitted", data: { id: "input-1" } })
    first.close()

    const second = new SqliteEventStore(path)
    expect(second.readAfter("session")).toEqual([
      expect.objectContaining({ sequence: 1, type: "input.admitted", data: { id: "input-1" } }),
    ])
    second.close()
  })

  test("fails closed when the stored schema is newer than the runtime", () => {
    const path = tempDatabase()
    const database = new Database(path, { create: true })
    database.exec(
      `CREATE TABLE runtime_schema_migrations (
        component TEXT PRIMARY KEY, version INTEGER NOT NULL, applied_at TEXT NOT NULL
      );`,
    )
    database
      .query("INSERT INTO runtime_schema_migrations(component, version, applied_at) VALUES (?, ?, ?)")
      .run("agent-events", 999, new Date().toISOString())
    database.close()
    expect(() => new SqliteEventStore(path)).toThrow(EventSchemaMigrationError)
  })
})

function tempDatabase(): string {
  const directory = mkdtempSync(join(tmpdir(), "jingwei-events-"))
  tempDirectories.push(directory)
  return join(directory, "runtime.db")
}

function removeTemporaryDirectory(directory: string): void {
  try {
    rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 20 })
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EBUSY") throw error
  }
}
