import { Database } from "bun:sqlite"
import type { ApprovalView } from "@jingwei/agent-protocol"

export interface ApprovalRepository {
  create(approval: ApprovalView): void
  get(id: string): ApprovalView | undefined
  list(sessionId?: string): ApprovalView[]
  resolve(id: string, status: Exclude<ApprovalView["status"], "pending">, decidedAt: string): ApprovalView
}

export class MemoryApprovalRepository implements ApprovalRepository {
  private readonly approvals = new Map<string, ApprovalView>()
  create(approval: ApprovalView): void {
    if (this.approvals.has(approval.id)) throw new Error(`Duplicate approval: ${approval.id}`)
    this.approvals.set(approval.id, structuredClone(approval))
  }
  get(id: string): ApprovalView | undefined {
    const approval = this.approvals.get(id)
    return approval ? structuredClone(approval) : undefined
  }
  list(sessionId?: string): ApprovalView[] {
    return [...this.approvals.values()]
      .filter((approval) => !sessionId || approval.sessionId === sessionId)
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt))
      .map((approval) => structuredClone(approval))
  }
  resolve(id: string, status: Exclude<ApprovalView["status"], "pending">, decidedAt: string): ApprovalView {
    const current = this.approvals.get(id)
    if (!current) throw new Error(`Approval not found: ${id}`)
    if (current.status !== "pending" && current.status !== status)
      throw new Error(`Approval already ${current.status}`)
    const resolved = { ...current, status, decidedAt }
    this.approvals.set(id, resolved)
    return structuredClone(resolved)
  }
}

export class SqliteApprovalRepository implements ApprovalRepository, Disposable {
  readonly database: Database
  constructor(path = ":memory:") {
    this.database = new Database(path, { create: true, strict: true })
    this.database.exec("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
    migrate(this.database)
  }
  create(approval: ApprovalView): void {
    this.database
      .query(
        `INSERT INTO runtime_approvals
          (id, session_id, run_id, tool_call_id, tool_name, permission, effects_json, resources_json,
           input_json, reason, scope_json, status, created_at, expires_at, decided_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        approval.id,
        approval.sessionId,
        approval.runId,
        approval.toolCallId,
        approval.toolName,
        approval.permission,
        JSON.stringify(approval.effects),
        JSON.stringify(approval.resourceClaims),
        JSON.stringify(approval.redactedInput),
        approval.reason,
        JSON.stringify(approval.scope),
        approval.status,
        approval.createdAt,
        approval.expiresAt,
        approval.decidedAt ?? null,
      )
  }
  get(id: string): ApprovalView | undefined {
    const row = this.database
      .query<ApprovalRow, [string]>("SELECT * FROM runtime_approvals WHERE id = ?")
      .get(id)
    return row ? toApproval(row) : undefined
  }
  list(sessionId?: string): ApprovalView[] {
    const rows = sessionId
      ? this.database
          .query<ApprovalRow, [string]>(
            "SELECT * FROM runtime_approvals WHERE session_id = ? ORDER BY created_at",
          )
          .all(sessionId)
      : this.database.query<ApprovalRow, []>("SELECT * FROM runtime_approvals ORDER BY created_at").all()
    return rows.map(toApproval)
  }
  resolve(id: string, status: Exclude<ApprovalView["status"], "pending">, decidedAt: string): ApprovalView {
    const current = this.get(id)
    if (!current) throw new Error(`Approval not found: ${id}`)
    if (current.status !== "pending" && current.status !== status)
      throw new Error(`Approval already ${current.status}`)
    this.database
      .query("UPDATE runtime_approvals SET status = ?, decided_at = ? WHERE id = ?")
      .run(status, decidedAt, id)
    return this.get(id) as ApprovalView
  }
  close(): void {
    this.database.close()
  }
  [Symbol.dispose](): void {
    this.close()
  }
}

interface ApprovalRow {
  id: string
  session_id: string
  run_id: string
  tool_call_id: string
  tool_name: string
  permission: string
  effects_json: string
  resources_json: string
  input_json: string
  reason: string
  scope_json: string
  status: ApprovalView["status"]
  created_at: string
  expires_at: string
  decided_at: string | null
}

function toApproval(row: ApprovalRow): ApprovalView {
  return {
    id: row.id,
    sessionId: row.session_id,
    runId: row.run_id,
    toolCallId: row.tool_call_id,
    toolName: row.tool_name,
    permission: row.permission,
    effects: JSON.parse(row.effects_json),
    resourceClaims: JSON.parse(row.resources_json),
    redactedInput: JSON.parse(row.input_json),
    reason: row.reason,
    scope: JSON.parse(row.scope_json),
    status: row.status,
    createdAt: row.created_at,
    expiresAt: row.expires_at,
    ...(row.decided_at ? { decidedAt: row.decided_at } : {}),
  }
}

function migrate(database: Database): void {
  database.exec(
    `CREATE TABLE IF NOT EXISTS runtime_approvals (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      run_id TEXT NOT NULL,
      tool_call_id TEXT NOT NULL,
      tool_name TEXT NOT NULL,
      permission TEXT NOT NULL,
      effects_json TEXT NOT NULL,
      resources_json TEXT NOT NULL,
      input_json TEXT NOT NULL,
      reason TEXT NOT NULL,
      scope_json TEXT NOT NULL,
      status TEXT NOT NULL,
      created_at TEXT NOT NULL,
      expires_at TEXT NOT NULL,
      decided_at TEXT
    );
    CREATE INDEX IF NOT EXISTS runtime_approvals_session_idx ON runtime_approvals(session_id, status);`,
  )
}
