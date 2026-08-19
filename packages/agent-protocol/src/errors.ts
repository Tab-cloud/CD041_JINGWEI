export class RuntimeConflictError extends Error {
  readonly code = "runtime-conflict"

  constructor(
    message: string,
    readonly conflict: "request-content-mismatch" | "stale-tool" | "stale-base" | "resource-conflict",
  ) {
    super(message)
    this.name = "RuntimeConflictError"
  }
}

export class RuntimeNotFoundError extends Error {
  readonly code = "runtime-not-found"

  constructor(entity: string, id: string) {
    super(`${entity} not found: ${id}`)
    this.name = "RuntimeNotFoundError"
  }
}

export class RuntimePermissionError extends Error {
  readonly code = "runtime-permission-denied"

  constructor(message: string) {
    super(message)
    this.name = "RuntimePermissionError"
  }
}

export class RuntimeQuarantinedError extends Error {
  readonly code = "runtime-safety-quarantine"

  constructor(message: string) {
    super(message)
    this.name = "RuntimeQuarantinedError"
  }
}
