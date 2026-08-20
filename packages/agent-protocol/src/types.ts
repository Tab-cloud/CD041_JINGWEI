import type {
  FindingSummary as GeneratedFindingSummary,
  JdpFailure as GeneratedJdpFailure,
  JdpRequest as GeneratedJdpRequest,
  JdpSuccess as GeneratedJdpSuccess,
  TraceContext as GeneratedTraceContext,
} from "@jingwei/jdp-bindings"

export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue }

export interface JsonSchema {
  $id?: string
  type?: string | string[]
  properties?: Record<string, JsonSchema>
  required?: string[]
  additionalProperties?: boolean | JsonSchema
  items?: JsonSchema
  enum?: JsonValue[]
  const?: JsonValue
  anyOf?: JsonSchema[]
  oneOf?: JsonSchema[]
  allOf?: JsonSchema[]
  minimum?: number
  maximum?: number
  minLength?: number
  maxLength?: number
  pattern?: string
  format?: string
  description?: string
}

export type DeliveryMode = "steer" | "queue"
export type RunKind = "root" | "specialist" | "workflow-node"
export type RunStatus =
  | "queued"
  | "running"
  | "waiting-approval"
  | "waiting-user"
  | "completed"
  | "failed"
  | "cancelled"
  | "quarantined"

export type ToolExecutionStatus =
  | "recorded"
  | "authorizing"
  | "waiting-approval"
  | "running"
  | "succeeded"
  | "failed"
  | "denied"
  | "unknown"

export type ModelAttemptStatus = "created" | "streaming" | "completed" | "failed" | "cancelled" | "discarded"
export type Effect =
  | "read"
  | "design-write"
  | "workspace-write"
  | "process"
  | "network"
  | "hardware"
  | "release"
export type PolicyDecision = "allow" | "ask" | "deny"

export interface CreateSessionInput {
  projectId: string
  workspaceId: string
  title?: string
  dataPolicy?: "local" | "organization" | "cloud-approved"
}

export interface SessionView {
  id: string
  projectId: string
  workspaceId: string
  title: string
  dataPolicy: "local" | "organization" | "cloud-approved"
  createdAt: string
  updatedAt: string
}

export interface AdmitInput {
  sessionId: string
  clientRequestId: string
  delivery: DeliveryMode
  content: string
  attachmentRefs?: string[]
}

export interface AdmittedInput extends AdmitInput {
  id: string
  sequence: number
  admittedAt: string
}

export interface RunView {
  id: string
  sessionId: string
  kind: RunKind
  ownerAgentId: string
  parentRunId?: string
  status: RunStatus
  output?: string
  error?: RuntimeFailure
  createdAt: string
  updatedAt: string
}

export interface RuntimeFailure {
  code: string
  stage: string
  summary: string
  fingerprint: string
  retryable: boolean
  objectRefs: string[]
  evidenceRefs: string[]
  suggestedActions: Array<{
    kind: "change-input" | "use-alternative" | "request-user" | "repair" | "abort-release"
    description: string
  }>
}

export interface ApprovalView {
  id: string
  sessionId: string
  runId: string
  toolCallId: string
  toolName: string
  permission: string
  effects: Effect[]
  resourceClaims: string[]
  redactedInput: Record<string, unknown>
  reason: string
  scope: ApprovalScope
  status: "pending" | "approved" | "denied" | "expired" | "cancelled"
  createdAt: string
  expiresAt: string
  decidedAt?: string
}

export interface ApprovalScope {
  sessionId: string
  permission: string
  resourceClaims: string[]
  expiresAt?: string
}

export interface ApprovalReply {
  approvalId: string
  decision: "allow-once" | "allow-scope" | "deny"
  scopeExpiresAt?: string
}

export interface ArtifactView {
  id: string
  sessionId: string
  mediaType: string
  byteLength: number
  sha256: string
  content?: string
  createdAt: string
}

export interface RuntimeCapabilities {
  protocolVersion: "0.1"
  deliveries: DeliveryMode[]
  durableEvents: true
  approvalScopes: true
  jdp: JdpCapabilities
  providers: ProviderCapabilities[]
}

export type TraceContext = GeneratedTraceContext

export interface DurableEvent<T = unknown> {
  schemaVersion: "0.1"
  aggregateId: string
  sequence: number
  timestamp: string
  sessionId?: string
  runId?: string
  taskId?: string
  traceId?: string
  type: string
  data: T
}

export interface RuntimeClient {
  createSession(input: CreateSessionInput): Promise<SessionView>
  admitInput(input: AdmitInput): Promise<AdmittedInput>
  resumeRun(sessionId: string): Promise<RunView>
  cancelRun(runId: string, reason?: string): Promise<void>
  events(sessionId: string, afterSequence?: number): AsyncIterable<DurableEvent>
  listApprovals(sessionId?: string): Promise<ApprovalView[]>
  replyApproval(input: ApprovalReply): Promise<void>
  getArtifact(artifactId: string): Promise<ArtifactView>
  capabilities(): Promise<RuntimeCapabilities>
}

export interface ProviderCapabilities {
  provider: string
  model: string
  nativeTools: boolean
  structuredOutput: boolean
  streaming: boolean
  reasoning: boolean
  image: boolean
  pdf: boolean
  contextTokens: number
  outputTokens: number
  cancellation: boolean
  retry: "none" | "safe" | "idempotent"
  idempotency: boolean
  faultDomain: string
  dataPolicy: Record<string, string | boolean>
}

/**
 * Internal desktop-to-runtime request used to validate a configured provider.
 * `apiKey` is transport-only secret material: callers must never persist, log,
 * emit, or return it.
 */
export interface ProviderConnectionTestInput {
  provider: "openai-responses"
  baseUrl: string
  model: string
  apiKey: string
  timeoutMs: number
}

export interface ProviderConnectionTestFailure {
  code:
    | "authentication-failed"
    | "endpoint-not-found"
    | "network-error"
    | "provider-unavailable"
    | "rate-limited"
    | "request-rejected"
    | "timeout"
    | "unsupported-provider"
    | "validation-failed"
    | "protocol-error"
  summary: string
  retryable: boolean
  httpStatus?: number
}

export interface ProviderConnectionTestResult {
  status: "passed" | "failed"
  provider: "openai-responses"
  protocol: "openai-responses"
  endpointHost: string
  model: string
  latencyMs: number
  testedAt: string
  usage?: {
    inputTokens: number
    outputTokens: number
  }
  error?: ProviderConnectionTestFailure
}

export type FindingSummary = GeneratedFindingSummary

export interface JdpCapabilities {
  protocolVersion: "0.1"
  operations: string[]
  reconciliation: boolean
}

export type JdpRequest<TInput> = GeneratedJdpRequest<TInput>

export type JdpSuccess<TOutput> = GeneratedJdpSuccess<TOutput>

export type JdpFailure = GeneratedJdpFailure

export interface ExecutionReconciliation {
  executionId: string
  idempotencyKey: string
  status: "not-found" | "running" | "succeeded" | "failed" | "unknown"
  responseHash?: string
  result?: JdpSuccess<unknown> | JdpFailure
  quarantineReason?: string
  repairActions?: string[]
}

export interface JdpClient {
  capabilities(): Promise<JdpCapabilities>
  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure>
  events(aggregateId: string, afterSequence?: number): AsyncIterable<DurableEvent>
  cancel(taskId: string): Promise<void>
  reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation>
}
