CREATE TABLE runtime_sessions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  title TEXT NOT NULL,
  data_policy TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE runtime_inputs (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES runtime_sessions(id),
  client_request_id TEXT NOT NULL,
  delivery TEXT NOT NULL,
  content TEXT NOT NULL,
  attachment_refs_json TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  input_sequence INTEGER NOT NULL,
  status TEXT NOT NULL,
  run_id TEXT,
  admitted_at TEXT NOT NULL,
  UNIQUE(session_id, client_request_id),
  UNIQUE(session_id, input_sequence)
);

CREATE TABLE runtime_runs (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES runtime_sessions(id),
  kind TEXT NOT NULL,
  owner_agent_id TEXT NOT NULL,
  parent_run_id TEXT,
  status TEXT NOT NULL,
  output TEXT,
  error_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE runtime_model_attempts (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  fault_domain TEXT NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  input_tokens INTEGER,
  output_tokens INTEGER,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE runtime_owner_lease (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  process_epoch TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  heartbeat_at INTEGER NOT NULL
);
