import { describe, expect, test } from "bun:test"
import { DurableEventBus, MemoryEventStore } from "@jingwei/agent-events"
import {
  type ApprovalRepository,
  ApprovalService,
  MemoryApprovalRepository,
  PolicyEngine,
  resolveAgentTools,
} from "../src/index.ts"

describe("policy boundaries", () => {
  test("deny remains the upper bound across layered rules", () => {
    const policy = new PolicyEngine([
      { id: "project-allow", action: "allow", projectId: "p", reason: "project permits reads" },
      {
        id: "tool-deny",
        action: "deny",
        projectId: "p",
        toolName: "design.query",
        reason: "session restriction",
      },
    ])
    expect(
      policy.evaluate({
        projectId: "p",
        sessionId: "s",
        agentId: "main-agent",
        agentRole: "primary",
        runKind: "root",
        toolName: "design.query",
        permission: "read-design",
        effects: ["read"],
        resourceClaims: ["project/p"],
      }),
    ).toEqual(expect.objectContaining({ action: "deny", matchedRuleIds: ["tool-deny"] }))
  })

  test("specialists cannot receive root-only orchestration tools", () => {
    expect(
      resolveAgentTools(
        {
          agentId: "pcb-engineer",
          role: "specialist",
          tools: ["design.query", "specialist.run", "workflow.start"],
          contextRefs: [],
          budget: { turns: 2, toolCalls: 3, outputTokens: 1_000 },
        },
        "specialist",
      ),
    ).toEqual(["design.query"])
  })
})

describe("durable approval", () => {
  test("scoped grants cannot escape session, permission, resources, or expiry", () => {
    const service = approvalService()
    const approval = service.request(requestInput())
    service.reply({ approvalId: approval.id, decision: "allow-scope" })

    expect(service.hasGrant("session", "network:supplier", ["supplier/lcsc"])).toBe(true)
    expect(service.hasGrant("other", "network:supplier", ["supplier/lcsc"])).toBe(false)
    expect(service.hasGrant("session", "release", ["supplier/lcsc"])).toBe(false)
    expect(service.hasGrant("session", "network:supplier", ["supplier/digikey"])).toBe(false)
  })

  test("request persistence failure prevents visibility and waiting", () => {
    const events = new DurableEventBus(new MemoryEventStore())
    const repository: ApprovalRepository = {
      create: () => {
        throw new Error("disk full")
      },
      get: () => undefined,
      list: () => [],
      resolve: () => {
        throw new Error("unreachable")
      },
    }
    const service = new ApprovalService(repository, events)
    expect(() => service.request(requestInput())).toThrow("disk full")
    expect(events.readAfter("session")).toEqual([])
  })

  test("decision persistence failure does not wake the waiter", async () => {
    const base = new MemoryApprovalRepository()
    const repository: ApprovalRepository = {
      create: (approval) => base.create(approval),
      get: (id) => base.get(id),
      list: (sessionId) => base.list(sessionId),
      resolve: () => {
        throw new Error("decision write failed")
      },
    }
    const service = new ApprovalService(repository, new DurableEventBus(new MemoryEventStore()))
    const approval = service.request(requestInput())
    const controller = new AbortController()
    const waiting = service.wait(approval.id, controller.signal)
    expect(() => service.reply({ approvalId: approval.id, decision: "allow-once" })).toThrow(
      "decision write failed",
    )
    controller.abort()
    await expect(waiting).rejects.toThrow()
    expect(base.get(approval.id)?.status).toBe("pending")
  })
})

function approvalService(): ApprovalService {
  return new ApprovalService(new MemoryApprovalRepository(), new DurableEventBus(new MemoryEventStore()))
}

function requestInput() {
  return {
    sessionId: "session",
    runId: "run",
    toolCallId: "call",
    toolName: "supplier.search",
    permission: "network:supplier",
    effects: ["network" as const],
    resourceClaims: ["supplier/lcsc"],
    input: { apiKey: "must-not-persist", query: "STM32" },
    reason: "External supplier call",
  }
}
