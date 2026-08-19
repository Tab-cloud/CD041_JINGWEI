const SECRET_KEY = /(?:api[-_]?key|authorization|cookie|credential|password|secret|token)/i
const PRIVATE_KEY = /(?:prompt|messages|jir|schematic|pcb|design|toolOutput|content)/i

export const REDACTED = "[REDACTED]"

export function redactRecord(input: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(input).map(([key, value]) => [key, redactValue(key, value)]))
}

function redactValue(key: string, value: unknown): unknown {
  if (SECRET_KEY.test(key) || PRIVATE_KEY.test(key)) return REDACTED
  if (Array.isArray(value)) return value.slice(0, 100).map((item) => redactValue("item", item))
  if (value && typeof value === "object") return redactRecord(value as Record<string, unknown>)
  if (typeof value === "string") return scrubText(value).slice(0, 2_000)
  return value
}

export function scrubText(value: string): string {
  return value
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, `Bearer ${REDACTED}`)
    .replace(/(?:sk|key|token)[-_][A-Za-z0-9_-]{12,}/gi, REDACTED)
}

export function safeTelemetryAttributes(
  input: Record<string, unknown>,
): Record<string, string | number | boolean> {
  const allowed =
    /^(?:session|run|tool|approval|provider|model|operation|request|execution|project|workspace|trace|span|status|error|duration|token|cost|effect|schema|protocol|fault)[A-Za-z0-9_.-]*$/i
  const output: Record<string, string | number | boolean> = {}
  for (const [key, value] of Object.entries(input)) {
    if (!allowed.test(key) || SECRET_KEY.test(key) || PRIVATE_KEY.test(key)) continue
    if (["string", "number", "boolean"].includes(typeof value))
      output[key] = value as string | number | boolean
  }
  return output
}
