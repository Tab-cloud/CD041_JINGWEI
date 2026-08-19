CREATE TABLE runtime_artifacts (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  media_type TEXT NOT NULL,
  byte_length INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
);
