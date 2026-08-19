import { Database } from "bun:sqlite"
import type { Effect, ToolExecutionStatus } from "@jingwei/agent-protocol"

export interface ToolExecutionView {
  id: string
  sessionId: string
  runId: string
  toolCallId: string
  toolName: string
  toolIdentity: string
  input: Record<string, unknown>
  inputHash: string
  idempotencyKey?: string
  effects: Effect[]
  resourceClaims: string[]
  authorization?: Record<string, unknown>
  status: ToolExecutionStatus
  output?: unknown
  artifactRefs: string[]
  error?: { code: string; summary: string; retryable: boolean }
  createdAt: string
  updatedAt: string
}

export interface ToolExecutionRepository {
  create(execution: ToolExecutionView): ToolExecutionView
  get(id: string): ToolExecutionView | undefined
  findIdempotent(sessionId: string, toolName: string, idempotencyKey: string): ToolExecutionView | undefined
  update(id: string, patch: Partial<ToolExecutionView>): ToolExecutionView
  listUnsettled(): ToolExecutionView[]
}

export class MemoryToolExecutionRepository implements ToolExecutionRepository {
  private readonly executions = new Map<string, ToolExecutionView>()
  create(execution: ToolExecutionView): ToolExecutionView {
    if (this.executions.has(execution.id)) throw new Error(`Duplicate execution: ${execution.id}`)
    this.executions.set(execution.id, structuredClone(execution))
    return structuredClone(execution)
  }
  get(id: string): ToolExecutionView | undefined {
    const execution = this.executions.get(id)
    return execution ? structuredClone(execution) : undefined
  }
  findIdempotent(sessionId: string, toolName: string, idempotencyKey: string): ToolExecutionView | undefined {
    const execution = [...this.executions.values()].find(
      (item) =>
        item.sessionId === sessionId && item.toolName === toolName && item.idempotencyKey === idempotencyKey,
    )
    return execution ? structuredClone(execution) : undefined
  }
  update(id: string, patch: Partial<ToolExecutionView>): ToolExecutionView {
    const current = this.executions.get(id)
    if (!current) throw new Error(`Execution not found: ${id}`)
    const next = { ...current, ...structuredClone(patch), id, updatedAt: new Date().toISOString() }
    this.executions.set(id, next)
    return structuredClone(next)
  }
  listUnsettled(): ToolExecutionView[] {
    return [...this.executions.values()]
      .filter((item) =>
        ["recorded", "authorizing", "waiting-approval", "running", "unknown"].includes(item.status),
      )
      .map((item) => structuredClone(item))
  }
}

export class SqliteToolExecutionRepository implements ToolExecutionRepository, Disposable {
  readonly database: Database
  constructor(path = ":memory:") {
    this.database = new Database(path, { create: true, strict: true })
    this.database.exec("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
    migrate(this.database)
  }
  create(execution: ToolExecutionView): ToolExecutionView {
    this.database
      .query(
        `INSERT INTO runtime_tool_executions
          (id, session_id, run_id, tool_call_id, tool_name, tool_identity, input_json, input_hash,
           idempotency_key, effects_json, resources_json, authorization_json, status, output_json,
           artifact_refs_json, error_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(...executionParams(execution))
    return execution
  }
  get(id: string): ToolExecutionView | undefined {
    const row = this.database
      .query<ExecutionRow, [string]>("SELECT * FROM runtime_tool_executions WHERE id = ?")
      .get(id)
    return row ? toExecution(row) : undefined
  }
  findIdempotent(sessionId: string, toolName: string, idempotencyKey: string): ToolExecutionView | undefined {
    const row = this.database
      .query<ExecutionRow, [string, string, string]>(
        `SELECT * FROM runtime_tool_executions
         WHERE session_id = ? AND tool_name = ? AND idempotency_key = ? LIMIT 1`,
      )
      .get(sessionId, toolName, idempotencyKey)
    return row ? toExecution(row) : undefined
  }
  update(id: string, patch: Partial<ToolExecutionView>): ToolExecutionView {
    const current = this.get(id)
    if (!current) throw new Error(`Execution not found: ${id}`)
    const next = { ...current, ...patch, id, updatedAt: new Date().toISOString() }
    this.database
      .query(
        `UPDATE runtime_tool_executions SET authorization_json = ?, status = ?, output_json = ?,
         artifact_refs_json = ?, error_json = ?, updated_at = ? WHERE id = ?`,
      )
      .run(
        next.authorization ? JSON.stringify(next.authorization) : null,
        next.status,
        next.output === undefined ? null : JSON.stringify(next.output),
        JSON.stringify(next.artifactRefs),
        next.error ? JSON.stringify(next.error) : null,
        next.updatedAt,
        id,
      )
    return this.get(id) as ToolExecutionView
  }
  listUnsettled(): ToolExecutionView[] {
    return this.database
      .query<ExecutionRow, []>(
        `SELECT * FROM runtime_tool_executions
         WHERE status IN ('recorded', 'authorizing', 'waiting-approval', 'running', 'unknown') ORDER BY created_at`,
      )
      .all()
      .map(toExecution)
  }
  close(): void {
    this.database.close()
  }
  [Symbol.dispose](): void {
    this.close()
  }
}

interface ExecutionRow {
  id: string
  session_id: string
  run_id: string
  tool_call_id: string
  tool_name: string
  tool_identity: string
  input_json: string
  input_hash: string
  idempotency_key: string | null
  effects_json: string
  resources_json: string
  authorization_json: string | null
  status: ToolExecutionStatus
  output_json: string | null
  artifact_refs_json: string
  error_json: string | null
  created_at: string
  updated_at: string
}

function toExecution(row: ExecutionRow): ToolExecutionView {
  return {
    id: row.id,
    sessionId: row.session_id,
    runId: row.run_id,
    toolCallId: row.tool_call_id,
    toolName: row.tool_name,
    toolIdentity: row.tool_identity,
    input: JSON.parse(row.input_json),
    inputHash: row.input_hash,
    ...(row.idempotency_key ? { idempotencyKey: row.idempotency_key } : {}),
    effects: JSON.parse(row.effects_json),
    resourceClaims: JSON.parse(row.resources_json),
    ...(row.authorization_json ? { authorization: JSON.parse(row.authorization_json) } : {}),
    status: row.status,
    ...(row.output_json ? { output: JSON.parse(row.output_json) } : {}),
    artifactRefs: JSON.parse(row.artifact_refs_json),
    ...(row.error_json ? { error: JSON.parse(row.error_json) } : {}),
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  }
}

function executionParams(execution: ToolExecutionView): Array<string | null> {
  return [
    execution.id,
    execution.sessionId,
    execution.runId,
    execution.toolCallId,
    execution.toolName,
    execution.toolIdentity,
    JSON.stringify(execution.input),
    execution.inputHash,
    execution.idempotencyKey ?? null,
    JSON.stringify(execution.effects),
    JSON.stringify(execution.resourceClaims),
    execution.authorization ? JSON.stringify(execution.authorization) : null,
    execution.status,
    execution.output === undefined ? null : JSON.stringify(execution.output),
    JSON.stringify(execution.artifactRefs),
    execution.error ? JSON.stringify(execution.error) : null,
    execution.createdAt,
    execution.updatedAt,
  ]
}

function migrate(database: Database): void {
  database.exec(
    `CREATE TABLE IF NOT EXISTS runtime_tool_executions (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      run_id TEXT NOT NULL,
      tool_call_id TEXT NOT NULL,
      tool_name TEXT NOT NULL,
      tool_identity TEXT NOT NULL,
      input_json TEXT NOT NULL,
      input_hash TEXT NOT NULL,
      idempotency_key TEXT,
      effects_json TEXT NOT NULL,
      resources_json TEXT NOT NULL,
      authorization_json TEXT,
      status TEXT NOT NULL,
      output_json TEXT,
      artifact_refs_json TEXT NOT NULL,
      error_json TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );
    CREATE UNIQUE INDEX IF NOT EXISTS runtime_tool_idempotency_idx
      ON runtime_tool_executions(session_id, tool_name, idempotency_key)
      WHERE idempotency_key IS NOT NULL;`,
  )
}
