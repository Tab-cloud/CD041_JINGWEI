CREATE TABLE runtime_events (
  aggregate_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK(sequence > 0),
  schema_version TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  session_id TEXT,
  run_id TEXT,
  task_id TEXT,
  trace_id TEXT,
  type TEXT NOT NULL,
  data_json TEXT NOT NULL,
  PRIMARY KEY (aggregate_id, sequence)
);

CREATE INDEX runtime_events_session_idx ON runtime_events(session_id, sequence);
