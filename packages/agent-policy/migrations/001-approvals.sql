CREATE TABLE runtime_approvals (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  tool_call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  permission TEXT NOT NULL,
  effects_json TEXT NOT NULL,
  resources_json TEXT NOT NULL,
  input_json TEXT NOT NULL,
  reason TEXT NOT NULL,
  scope_json TEXT NOT NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  decided_at TEXT
);
