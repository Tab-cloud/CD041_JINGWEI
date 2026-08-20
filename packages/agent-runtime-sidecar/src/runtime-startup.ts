import {
  type RecoveryAction,
  type RecoveryService,
  RuntimeOwnerLeaseConflictError,
} from "@jingwei/agent-runtime"

export interface RecoveryLeaseWaitOptions {
  timeoutMs?: number
  retryDelayMs?: number
  now?: () => number
  sleep?: (milliseconds: number) => Promise<void>
}

export async function recoverWhenOwnerLeaseAvailable(
  recovery: Pick<RecoveryService, "recover">,
  options: RecoveryLeaseWaitOptions = {},
): Promise<RecoveryAction[]> {
  const timeoutMs = options.timeoutMs ?? 20_000
  const retryDelayMs = options.retryDelayMs ?? 250
  const now = options.now ?? Date.now
  const sleep = options.sleep ?? ((milliseconds: number) => Bun.sleep(milliseconds))
  const deadline = now() + timeoutMs

  for (;;) {
    try {
      return await recovery.recover()
    } catch (error) {
      if (!(error instanceof RuntimeOwnerLeaseConflictError)) throw error
      const remaining = deadline - now()
      if (remaining <= 0) {
        throw new RuntimeOwnerLeaseConflictError(
          `Runtime recovery lease remained owned after ${timeoutMs} ms`,
        )
      }
      await sleep(Math.min(retryDelayMs, remaining))
    }
  }
}
