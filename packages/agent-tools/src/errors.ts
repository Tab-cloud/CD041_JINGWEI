export class UnknownToolSideEffectError extends Error {
  constructor(
    message: string,
    readonly reconciliation: { executionId?: string; idempotencyKey?: string } = {},
  ) {
    super(message)
    this.name = "UnknownToolSideEffectError"
  }
}

export class ToolOutputTooLargeError extends Error {
  constructor(readonly byteLength: number) {
    super(`Tool output exceeds inline budget: ${byteLength} bytes`)
    this.name = "ToolOutputTooLargeError"
  }
}
