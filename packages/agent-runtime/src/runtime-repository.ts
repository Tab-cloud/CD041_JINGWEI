import { Database } from "bun:sqlite"
import { createHash } from "node:crypto"
import type {
  AdmitInput,
  AdmittedInput,
  CreateSessionInput,
  ModelAttemptStatus,
  RunKind,
  RunStatus,
  RuntimeFailure,
  RunView,
  SessionView,
} from "@jingwei/agent-protocol"
import { RuntimeConflictError, RuntimeNotFoundError } from "@jingwei/agent-protocol"

export interface RuntimeInputRecord extends AdmittedInput {
  status: "pending" | "consumed" | "cancelled"
  runId?: string
}

export interface ModelAttemptRecord {
  id: string
  sessionId: string
  runId: string
  provider: string
  model: string
  faultDomain: string
  status: ModelAttemptStatus
  error?: string
  inputTokens?: number
  outputTokens?: number
  createdAt: string
  updatedAt: string
}

export interface RuntimeRepository {
  createSession(input: CreateSessionInput): SessionView
  getSession(id: string): SessionView | undefined
  admitInput(input: AdmitInput): RuntimeInputRecord
  listPendingInputs(sessionId: string): RuntimeInputRecord[]
  listInputsForRun(runId: string): RuntimeInputRecord[]
  consumeInput(inputId: string, runId: string): RuntimeInputRecord
  createRun(sessionId: string, kind: RunKind, ownerAgentId: string, parentRunId?: string): RunView
  getRun(id: string): RunView | undefined
  findActiveRun(sessionId: string): RunView | undefined
  listRecoverableRuns(): RunView[]
  updateRun(id: string, patch: { status?: RunStatus; output?: string; error?: RuntimeFailure }): RunView
  createModelAttempt(input: Omit<ModelAttemptRecord, "id" | "createdAt" | "updatedAt">): ModelAttemptRecord
  updateModelAttempt(
    id: string,
    patch: Partial<Pick<ModelAttemptRecord, "status" | "error" | "inputTokens" | "outputTokens">>,
  ): ModelAttemptRecord
  listIncompleteAttempts(): ModelAttemptRecord[]
  acquireOwnerLease(processEpoch: string, ttlMs: number): boolean
  heartbeatOwnerLease(processEpoch: string, ttlMs: number): boolean
  releaseOwnerLease(processEpoch: string): boolean
}

export class SqliteRuntimeRepository implements RuntimeRepository, Disposable {
  readonly database: Database

  constructor(path = ":memory:") {
    this.database = new Database(path, { create: true, strict: true })
    this.database.exec("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
    migrate(this.database)
  }

  createSession(input: CreateSessionInput): SessionView {
    const now = new Date().toISOString()
    const session: SessionView = {
      id: crypto.randomUUID(),
      projectId: input.projectId,
      workspaceId: input.workspaceId,
      title: input.title?.trim().slice(0, 200) || "New session",
      dataPolicy: input.dataPolicy ?? "local",
      createdAt: now,
      updatedAt: now,
    }
    this.database
      .query(
        `INSERT INTO runtime_sessions
          (id, project_id, workspace_id, title, data_policy, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        session.id,
        session.projectId,
        session.workspaceId,
        session.title,
        session.dataPolicy,
        session.createdAt,
        session.updatedAt,
      )
    return session
  }

  getSession(id: string): SessionView | undefined {
    const row = this.database
      .query<SessionRow, [string]>("SELECT * FROM runtime_sessions WHERE id = ?")
      .get(id)
    return row ? toSession(row) : undefined
  }

  admitInput(input: AdmitInput): RuntimeInputRecord {
    return this.database.transaction(() => {
      if (!this.getSession(input.sessionId)) throw new RuntimeNotFoundError("session", input.sessionId)
      const existing = this.database
        .query<InputRow, [string, string]>(
          "SELECT * FROM runtime_inputs WHERE session_id = ? AND client_request_id = ?",
        )
        .get(input.sessionId, input.clientRequestId)
      const normalized = normalizeAdmission(input)
      const inputHash = hashAdmission(normalized)
      if (existing) {
        if (existing.input_hash !== inputHash) {
          throw new RuntimeConflictError(
            "clientRequestId was reused with different content or delivery semantics",
            "request-content-mismatch",
          )
        }
        return toInput(existing)
      }
      const sequence =
        (this.database
          .query<{ sequence: number }, [string]>(
            "SELECT COALESCE(MAX(input_sequence), 0) AS sequence FROM runtime_inputs WHERE session_id = ?",
          )
          .get(input.sessionId)?.sequence ?? 0) + 1
      const admittedAt = new Date().toISOString()
      const record: RuntimeInputRecord = {
        id: crypto.randomUUID(),
        ...normalized,
        sequence,
        admittedAt,
        status: "pending",
      }
      this.database
        .query(
          `INSERT INTO runtime_inputs
            (id, session_id, client_request_id, delivery, content, attachment_refs_json, input_hash,
             input_sequence, status, run_id, admitted_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        )
        .run(
          record.id,
          record.sessionId,
          record.clientRequestId,
          record.delivery,
          record.content,
          JSON.stringify(record.attachmentRefs ?? []),
          inputHash,
          record.sequence,
          record.status,
          null,
          record.admittedAt,
        )
      return record
    })()
  }

  listPendingInputs(sessionId: string): RuntimeInputRecord[] {
    return this.database
      .query<InputRow, [string]>(
        "SELECT * FROM runtime_inputs WHERE session_id = ? AND status = 'pending' ORDER BY input_sequence",
      )
      .all(sessionId)
      .map(toInput)
  }

  listInputsForRun(runId: string): RuntimeInputRecord[] {
    return this.database
      .query<InputRow, [string]>("SELECT * FROM runtime_inputs WHERE run_id = ? ORDER BY input_sequence")
      .all(runId)
      .map(toInput)
  }

  consumeInput(inputId: string, runId: string): RuntimeInputRecord {
    const result = this.database
      .query("UPDATE runtime_inputs SET status = 'consumed', run_id = ? WHERE id = ? AND status = 'pending'")
      .run(runId, inputId)
    if (result.changes !== 1) throw new Error(`Input is not pending: ${inputId}`)
    const row = this.database
      .query<InputRow, [string]>("SELECT * FROM runtime_inputs WHERE id = ?")
      .get(inputId)
    return toInput(row as InputRow)
  }

  createRun(sessionId: string, kind: RunKind, ownerAgentId: string, parentRunId?: string): RunView {
    if (!this.getSession(sessionId)) throw new RuntimeNotFoundError("session", sessionId)
    if (kind === "root" && ownerAgentId !== "main-agent")
      throw new Error("Root run must be owned by main-agent")
    const now = new Date().toISOString()
    const run: RunView = {
      id: crypto.randomUUID(),
      sessionId,
      kind,
      ownerAgentId,
      ...(parentRunId ? { parentRunId } : {}),
      status: "queued",
      createdAt: now,
      updatedAt: now,
    }
    this.database
      .query(
        `INSERT INTO runtime_runs
          (id, session_id, kind, owner_agent_id, parent_run_id, status, output, error_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        run.id,
        run.sessionId,
        run.kind,
        run.ownerAgentId,
        run.parentRunId ?? null,
        run.status,
        null,
        null,
        run.createdAt,
        run.updatedAt,
      )
    return run
  }

  getRun(id: string): RunView | undefined {
    const row = this.database.query<RunRow, [string]>("SELECT * FROM runtime_runs WHERE id = ?").get(id)
    return row ? toRun(row) : undefined
  }

  findActiveRun(sessionId: string): RunView | undefined {
    const row = this.database
      .query<RunRow, [string]>(
        `SELECT * FROM runtime_runs WHERE session_id = ?
         AND status IN ('queued', 'running', 'waiting-approval', 'waiting-user') ORDER BY created_at DESC LIMIT 1`,
      )
      .get(sessionId)
    return row ? toRun(row) : undefined
  }

  listRecoverableRuns(): RunView[] {
    return this.database
      .query<RunRow, []>(
        `SELECT * FROM runtime_runs
         WHERE status IN ('queued', 'running', 'waiting-approval', 'waiting-user') ORDER BY created_at`,
      )
      .all()
      .map(toRun)
  }

  updateRun(id: string, patch: { status?: RunStatus; output?: string; error?: RuntimeFailure }): RunView {
    const current = this.getRun(id)
    if (!current) throw new RuntimeNotFoundError("run", id)
    const next = { ...current, ...patch, id, updatedAt: new Date().toISOString() }
    this.database
      .query("UPDATE runtime_runs SET status = ?, output = ?, error_json = ?, updated_at = ? WHERE id = ?")
      .run(
        next.status,
        next.output ?? null,
        next.error ? JSON.stringify(next.error) : null,
        next.updatedAt,
        id,
      )
    return this.getRun(id) as RunView
  }

  createModelAttempt(input: Omit<ModelAttemptRecord, "id" | "createdAt" | "updatedAt">): ModelAttemptRecord {
    const now = new Date().toISOString()
    const attempt: ModelAttemptRecord = { id: crypto.randomUUID(), ...input, createdAt: now, updatedAt: now }
    this.database
      .query(
        `INSERT INTO runtime_model_attempts
          (id, session_id, run_id, provider, model, fault_domain, status, error, input_tokens,
           output_tokens, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        attempt.id,
        attempt.sessionId,
        attempt.runId,
        attempt.provider,
        attempt.model,
        attempt.faultDomain,
        attempt.status,
        attempt.error ?? null,
        attempt.inputTokens ?? null,
        attempt.outputTokens ?? null,
        attempt.createdAt,
        attempt.updatedAt,
      )
    return attempt
  }

  updateModelAttempt(
    id: string,
    patch: Partial<Pick<ModelAttemptRecord, "status" | "error" | "inputTokens" | "outputTokens">>,
  ): ModelAttemptRecord {
    const row = this.database
      .query<ModelAttemptRow, [string]>("SELECT * FROM runtime_model_attempts WHERE id = ?")
      .get(id)
    if (!row) throw new RuntimeNotFoundError("model attempt", id)
    const current = toModelAttempt(row)
    const next = { ...current, ...patch, updatedAt: new Date().toISOString() }
    this.database
      .query(
        `UPDATE runtime_model_attempts SET status = ?, error = ?, input_tokens = ?, output_tokens = ?, updated_at = ?
         WHERE id = ?`,
      )
      .run(
        next.status,
        next.error ?? null,
        next.inputTokens ?? null,
        next.outputTokens ?? null,
        next.updatedAt,
        id,
      )
    return next
  }

  listIncompleteAttempts(): ModelAttemptRecord[] {
    return this.database
      .query<ModelAttemptRow, []>(
        "SELECT * FROM runtime_model_attempts WHERE status IN ('created', 'streaming') ORDER BY created_at",
      )
      .all()
      .map(toModelAttempt)
  }

  acquireOwnerLease(processEpoch: string, ttlMs: number): boolean {
    return this.database.transaction(() => {
      const current = this.database
        .query<{ process_epoch: string; expires_at: number }, []>(
          "SELECT process_epoch, expires_at FROM runtime_owner_lease WHERE singleton = 1",
        )
        .get()
      const now = Date.now()
      if (current && current.process_epoch !== processEpoch && current.expires_at > now) return false
      this.database
        .query(
          `INSERT INTO runtime_owner_lease(singleton, process_epoch, expires_at, heartbeat_at)
           VALUES (1, ?, ?, ?)
           ON CONFLICT(singleton) DO UPDATE SET process_epoch = excluded.process_epoch,
             expires_at = excluded.expires_at, heartbeat_at = excluded.heartbeat_at`,
        )
        .run(processEpoch, now + ttlMs, now)
      return true
    })()
  }

  heartbeatOwnerLease(processEpoch: string, ttlMs: number): boolean {
    const now = Date.now()
    const result = this.database
      .query(
        `UPDATE runtime_owner_lease SET expires_at = ?, heartbeat_at = ?
         WHERE singleton = 1 AND process_epoch = ?`,
      )
      .run(now + ttlMs, now, processEpoch)
    return result.changes === 1
  }

  releaseOwnerLease(processEpoch: string): boolean {
    const result = this.database
      .query("DELETE FROM runtime_owner_lease WHERE singleton = 1 AND process_epoch = ?")
      .run(processEpoch)
    return result.changes === 1
  }

  close(): void {
    this.database.close()
  }

  [Symbol.dispose](): void {
    this.close()
  }
}

interface SessionRow {
  id: string
  project_id: string
  workspace_id: string
  title: string
  data_policy: SessionView["dataPolicy"]
  created_at: string
  updated_at: string
}

interface InputRow {
  id: string
  session_id: string
  client_request_id: string
  delivery: AdmitInput["delivery"]
  content: string
  attachment_refs_json: string
  input_hash: string
  input_sequence: number
  status: RuntimeInputRecord["status"]
  run_id: string | null
  admitted_at: string
}

interface RunRow {
  id: string
  session_id: string
  kind: RunKind
  owner_agent_id: string
  parent_run_id: string | null
  status: RunStatus
  output: string | null
  error_json: string | null
  created_at: string
  updated_at: string
}

interface ModelAttemptRow {
  id: string
  session_id: string
  run_id: string
  provider: string
  model: string
  fault_domain: string
  status: ModelAttemptStatus
  error: string | null
  input_tokens: number | null
  output_tokens: number | null
  created_at: string
  updated_at: string
}

function toSession(row: SessionRow): SessionView {
  return {
    id: row.id,
    projectId: row.project_id,
    workspaceId: row.workspace_id,
    title: row.title,
    dataPolicy: row.data_policy,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  }
}

function toInput(row: InputRow): RuntimeInputRecord {
  return {
    id: row.id,
    sessionId: row.session_id,
    clientRequestId: row.client_request_id,
    delivery: row.delivery,
    content: row.content,
    attachmentRefs: JSON.parse(row.attachment_refs_json),
    sequence: row.input_sequence,
    admittedAt: row.admitted_at,
    status: row.status,
    ...(row.run_id ? { runId: row.run_id } : {}),
  }
}

function toRun(row: RunRow): RunView {
  return {
    id: row.id,
    sessionId: row.session_id,
    kind: row.kind,
    ownerAgentId: row.owner_agent_id,
    ...(row.parent_run_id ? { parentRunId: row.parent_run_id } : {}),
    status: row.status,
    ...(row.output ? { output: row.output } : {}),
    ...(row.error_json ? { error: JSON.parse(row.error_json) } : {}),
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  }
}

function toModelAttempt(row: ModelAttemptRow): ModelAttemptRecord {
  return {
    id: row.id,
    sessionId: row.session_id,
    runId: row.run_id,
    provider: row.provider,
    model: row.model,
    faultDomain: row.fault_domain,
    status: row.status,
    ...(row.error ? { error: row.error } : {}),
    ...(row.input_tokens !== null ? { inputTokens: row.input_tokens } : {}),
    ...(row.output_tokens !== null ? { outputTokens: row.output_tokens } : {}),
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  }
}

function normalizeAdmission(input: AdmitInput): AdmitInput {
  const content = input.content.trim()
  if (!content) throw new Error("Input content must not be empty")
  if (!/^[A-Za-z0-9._:-]{1,200}$/.test(input.clientRequestId)) throw new Error("Invalid clientRequestId")
  return {
    sessionId: input.sessionId,
    clientRequestId: input.clientRequestId,
    delivery: input.delivery,
    content,
    attachmentRefs: [...new Set(input.attachmentRefs ?? [])],
  }
}

function hashAdmission(input: AdmitInput): string {
  return createHash("sha256")
    .update(JSON.stringify([input.delivery, input.content, input.attachmentRefs ?? []]))
    .digest("hex")
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
    const version = database
      .query<{ version: number }, [string]>(
        "SELECT version FROM runtime_schema_migrations WHERE component = ?",
      )
      .get("agent-runtime")?.version
    if (version !== undefined && version > 1)
      throw new Error(`Unsupported runtime schema version: ${version}`)
    if ((version ?? 0) === 0) {
      database.transaction(() => {
        database.exec(RUNTIME_SCHEMA_V1)
        database
          .query(
            `INSERT INTO runtime_schema_migrations(component, version, applied_at)
             VALUES (?, ?, ?) ON CONFLICT(component) DO UPDATE SET version = excluded.version, applied_at = excluded.applied_at`,
          )
          .run("agent-runtime", 1, new Date().toISOString())
      })()
    }
  } catch (error) {
    throw new Error("Runtime schema migration failed; no model execution may start", { cause: error })
  }
}

const RUNTIME_SCHEMA_V1 = `
CREATE TABLE runtime_sessions (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, workspace_id TEXT NOT NULL, title TEXT NOT NULL,
  data_policy TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE runtime_inputs (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES runtime_sessions(id), client_request_id TEXT NOT NULL,
  delivery TEXT NOT NULL, content TEXT NOT NULL, attachment_refs_json TEXT NOT NULL, input_hash TEXT NOT NULL,
  input_sequence INTEGER NOT NULL, status TEXT NOT NULL, run_id TEXT, admitted_at TEXT NOT NULL,
  UNIQUE(session_id, client_request_id), UNIQUE(session_id, input_sequence)
);
CREATE TABLE runtime_runs (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES runtime_sessions(id), kind TEXT NOT NULL,
  owner_agent_id TEXT NOT NULL, parent_run_id TEXT, status TEXT NOT NULL, output TEXT, error_json TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX runtime_runs_session_idx ON runtime_runs(session_id, status);
CREATE TABLE runtime_model_attempts (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL, run_id TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
  fault_domain TEXT NOT NULL, status TEXT NOT NULL, error TEXT, input_tokens INTEGER, output_tokens INTEGER,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE runtime_owner_lease (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1), process_epoch TEXT NOT NULL,
  expires_at INTEGER NOT NULL, heartbeat_at INTEGER NOT NULL
);`
