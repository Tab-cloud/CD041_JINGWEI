import {
  JDP_PROTOCOL_VERSION,
  type EditCommandGroup,
  type JdpRequest,
  type ViewRequest,
} from "../src/generated.js";

const request: JdpRequest<{ revision: string }> = {
  protocolVersion: JDP_PROTOCOL_VERSION,
  requestId: "request-fixture",
  idempotencyKey: "idem-fixture",
  workspaceId: "ws-fixture",
  projectId: "project-fixture",
  operation: "design.validate",
  dryRun: false,
  input: { revision: "rev-fixture" },
};

const edit: EditCommandGroup = {
  editSessionId: "session-fixture",
  baseDraftRevision: "rev-draft",
  commands: [{ kind: "core.set_property", payload: { key: "label", value: "U1" } }],
  expectedPostconditions: ["graph.integrity"],
};

const view: ViewRequest = {
  revision: "rev-fixture",
  viewport: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
  layers: ["schematic"],
  levelOfDetail: 1,
  selection: [],
};

void [request, edit, view];
