import { Database } from "bun:sqlite"
import { createHash } from "node:crypto"
import type { ArtifactView } from "@jingwei/agent-protocol"

export interface ArtifactStore {
  create(sessionId: string, mediaType: string, content: string): ArtifactView
  get(id: string): ArtifactView | undefined
}

export class MemoryArtifactStore implements ArtifactStore {
  private readonly artifacts = new Map<string, ArtifactView>()
  create(sessionId: string, mediaType: string, content: string): ArtifactView {
    const artifact: ArtifactView = {
      id: crypto.randomUUID(),
      sessionId,
      mediaType,
      byteLength: Buffer.byteLength(content),
      sha256: createHash("sha256").update(content).digest("hex"),
      content,
      createdAt: new Date().toISOString(),
    }
    this.artifacts.set(artifact.id, artifact)
    return structuredClone(artifact)
  }
  get(id: string): ArtifactView | undefined {
    const artifact = this.artifacts.get(id)
    return artifact ? structuredClone(artifact) : undefined
  }
}

export class SqliteArtifactStore implements ArtifactStore, Disposable {
  readonly database: Database
  constructor(path = ":memory:") {
    this.database = new Database(path, { create: true, strict: true })
    this.database.exec(
      `PRAGMA journal_mode = WAL;
       CREATE TABLE IF NOT EXISTS runtime_artifacts (
         id TEXT PRIMARY KEY, session_id TEXT NOT NULL, media_type TEXT NOT NULL, byte_length INTEGER NOT NULL,
         sha256 TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL
       );`,
    )
  }
  create(sessionId: string, mediaType: string, content: string): ArtifactView {
    const artifact: ArtifactView = {
      id: crypto.randomUUID(),
      sessionId,
      mediaType,
      byteLength: Buffer.byteLength(content),
      sha256: createHash("sha256").update(content).digest("hex"),
      content,
      createdAt: new Date().toISOString(),
    }
    this.database
      .query(
        `INSERT INTO runtime_artifacts(id, session_id, media_type, byte_length, sha256, content, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        artifact.id,
        artifact.sessionId,
        artifact.mediaType,
        artifact.byteLength,
        artifact.sha256,
        content,
        artifact.createdAt,
      )
    return artifact
  }
  get(id: string): ArtifactView | undefined {
    const row = this.database
      .query<
        {
          id: string
          session_id: string
          media_type: string
          byte_length: number
          sha256: string
          content: string
          created_at: string
        },
        [string]
      >("SELECT * FROM runtime_artifacts WHERE id = ?")
      .get(id)
    return row
      ? {
          id: row.id,
          sessionId: row.session_id,
          mediaType: row.media_type,
          byteLength: row.byte_length,
          sha256: row.sha256,
          content: row.content,
          createdAt: row.created_at,
        }
      : undefined
  }
  close(): void {
    this.database.close()
  }
  [Symbol.dispose](): void {
    this.close()
  }
}
