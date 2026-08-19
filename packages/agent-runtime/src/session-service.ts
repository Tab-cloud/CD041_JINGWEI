import type { DurableEventBus } from "@jingwei/agent-events"
import type { AdmitInput, AdmittedInput, CreateSessionInput, SessionView } from "@jingwei/agent-protocol"
import type { RuntimeRepository } from "./runtime-repository.ts"

export class SessionService {
  constructor(
    private readonly repository: RuntimeRepository,
    private readonly events: DurableEventBus,
  ) {}

  create(input: CreateSessionInput): SessionView {
    const session = this.repository.createSession(input)
    this.events.append({
      aggregateId: session.id,
      sessionId: session.id,
      type: "session.created",
      data: session,
    })
    return session
  }

  get(sessionId: string): SessionView | undefined {
    return this.repository.getSession(sessionId)
  }

  admit(input: AdmitInput): AdmittedInput {
    const admitted = this.repository.admitInput(input)
    const alreadyPublished = this.events
      .readAfter(input.sessionId)
      .some(
        (event) =>
          event.type === "input.admitted" &&
          (event.data as { clientRequestId?: string }).clientRequestId === input.clientRequestId,
      )
    if (!alreadyPublished) {
      this.events.append({
        aggregateId: input.sessionId,
        sessionId: input.sessionId,
        type: "input.admitted",
        data: {
          id: admitted.id,
          clientRequestId: admitted.clientRequestId,
          delivery: admitted.delivery,
          inputSequence: admitted.sequence,
          attachmentRefs: admitted.attachmentRefs ?? [],
        },
      })
    }
    return admitted
  }
}
