import type {
  ProviderConnectionTestFailure,
  ProviderConnectionTestInput,
  ProviderConnectionTestResult,
} from "@jingwei/agent-protocol"
import { OpenAIResponsesProvider, type ProviderFetch } from "./openai-responses-provider.ts"
import { collectProviderTurn } from "./provider.ts"

const MIN_TIMEOUT_MS = 5_000
const MAX_TIMEOUT_MS = 60_000

/**
 * Performs a bounded, side-effect-free Responses request. The credential is
 * constructor-only and the returned evidence is deliberately content-free.
 */
export async function testProviderConnection(
  input: ProviderConnectionTestInput,
  dependencies: { fetch?: ProviderFetch } = {},
): Promise<ProviderConnectionTestResult> {
  const startedAt = performance.now()
  const testedAt = new Date().toISOString()
  const endpoint = validateEndpoint(input.baseUrl)
  const evidence = {
    provider: "openai-responses" as const,
    protocol: "openai-responses" as const,
    endpointHost: endpoint?.host ?? "invalid-endpoint",
    model: safeModel(input.model),
    testedAt,
  }

  const validationFailure = validateInput(input, endpoint)
  if (validationFailure || !endpoint) {
    return {
      ...evidence,
      status: "failed",
      latencyMs: elapsed(startedAt),
      error: validationFailure ?? failure("validation-failed", "API Base URL 无效。", false),
    }
  }

  try {
    const provider = new OpenAIResponsesProvider({
      apiKey: input.apiKey,
      model: input.model,
      baseURL: endpoint.toString().replace(/\/$/u, ""),
      contextTokens: 8_192,
      outputTokens: 32,
      timeoutMs: input.timeoutMs,
      store: false,
      reasoning: false,
      image: false,
      pdf: false,
      structuredOutput: false,
      dataPolicy: { purpose: "provider-connection-test", responseContentReturned: false },
      ...(dependencies.fetch ? { fetch: dependencies.fetch } : {}),
    })
    const completed = await collectProviderTurn(provider, {
      messages: [{ role: "user", content: "Reply with exactly JINGWEI_PROVIDER_TEST_OK" }],
      tools: [],
      signal: AbortSignal.timeout(input.timeoutMs),
      maxOutputTokens: 32,
    })
    return {
      ...evidence,
      status: "passed",
      latencyMs: elapsed(startedAt),
      ...(completed.usage ? { usage: completed.usage } : {}),
    }
  } catch (error) {
    return {
      ...evidence,
      status: "failed",
      latencyMs: elapsed(startedAt),
      error: classifyFailure(error),
    }
  }
}

function validateInput(
  input: ProviderConnectionTestInput,
  endpoint: URL | undefined,
): ProviderConnectionTestFailure | undefined {
  if (input.provider !== "openai-responses") {
    return failure("unsupported-provider", "当前 Runtime 只支持 OpenAI Responses 连接测试。", false)
  }
  if (!endpoint) {
    return failure("validation-failed", "API Base URL 必须是无凭据、无查询参数的 HTTPS 地址。", false)
  }
  if (!input.model || input.model.length > 128 || !/^[-A-Za-z0-9._/:]+$/u.test(input.model)) {
    return failure("validation-failed", "模型 ID 格式无效。", false)
  }
  if (
    input.apiKey !== input.apiKey.trim() ||
    input.apiKey.length < 8 ||
    input.apiKey.length > 2_560 ||
    !/^[\x20-\x7E]+$/u.test(input.apiKey)
  ) {
    return failure("validation-failed", "API Key 格式无效。", false)
  }
  if (
    !Number.isInteger(input.timeoutMs) ||
    input.timeoutMs < MIN_TIMEOUT_MS ||
    input.timeoutMs > MAX_TIMEOUT_MS
  ) {
    return failure("validation-failed", "连接测试超时必须在 5–60 秒之间。", false)
  }
  return undefined
}

function validateEndpoint(value: string): URL | undefined {
  try {
    const endpoint = new URL(value)
    if (
      endpoint.protocol !== "https:" ||
      !endpoint.hostname ||
      endpoint.username ||
      endpoint.password ||
      endpoint.search ||
      endpoint.hash
    ) {
      return undefined
    }
    return endpoint
  } catch {
    return undefined
  }
}

function classifyFailure(error: unknown): ProviderConnectionTestFailure {
  const status = httpStatus(error)
  if (isTimeout(error)) return failure("timeout", "连接测试超时；请检查网络或增大上游响应速度。", true)
  if (status === 401 || status === 403) {
    return failure("authentication-failed", "端点拒绝了凭据；请检查 API Key 与账号权限。", false, status)
  }
  if (status === 404) {
    return failure("endpoint-not-found", "端点或模型不存在；请检查 Base URL 与模型 ID。", false, status)
  }
  if (status === 429) return failure("rate-limited", "端点当前触发限流或额度限制。", true, status)
  if (status !== undefined && status >= 500) {
    return failure("provider-unavailable", "模型服务暂时不可用。", true, status)
  }
  if (status === 400 || status === 409 || status === 422) {
    return failure(
      "request-rejected",
      "端点拒绝了 Responses 测试请求；请确认该服务兼容 Responses API。",
      false,
      status,
    )
  }
  if (isNetworkError(error))
    return failure("network-error", "无法连接模型端点；请检查网络、代理与 DNS。", true)
  return failure("protocol-error", "端点未返回有效的 OpenAI Responses 流。", false, status)
}

function httpStatus(error: unknown): number | undefined {
  if (!error || typeof error !== "object" || !("status" in error)) return undefined
  const value = (error as { status?: unknown }).status
  return typeof value === "number" && Number.isInteger(value) ? value : undefined
}

function isTimeout(error: unknown): boolean {
  if (!error || typeof error !== "object") return false
  const name = "name" in error ? String((error as { name?: unknown }).name) : ""
  const code = "code" in error ? String((error as { code?: unknown }).code) : ""
  return (
    name === "AbortError" ||
    name === "TimeoutError" ||
    name === "APIConnectionTimeoutError" ||
    code === "ETIMEDOUT"
  )
}

function isNetworkError(error: unknown): boolean {
  if (error instanceof TypeError) return true
  if (!error || typeof error !== "object") return false
  const name = "name" in error ? String((error as { name?: unknown }).name) : ""
  if (name === "APIConnectionError") return true
  if (!("code" in error)) return false
  return ["ECONNREFUSED", "ECONNRESET", "ENETUNREACH", "ENOTFOUND"].includes(
    String((error as { code?: unknown }).code),
  )
}

function failure(
  code: ProviderConnectionTestFailure["code"],
  summary: string,
  retryable: boolean,
  httpStatus?: number,
): ProviderConnectionTestFailure {
  return { code, summary, retryable, ...(httpStatus === undefined ? {} : { httpStatus }) }
}

function elapsed(startedAt: number): number {
  return Math.max(0, Math.round(performance.now() - startedAt))
}

function safeModel(value: string): string {
  return /^[-A-Za-z0-9._/:]{1,128}$/u.test(value) ? value : "invalid-model"
}
