# Jingwei Agent Windows Alpha 0.1.1

## Purpose

This release eliminates ambiguity with the historical `Jingwei Agent Dev`/`MOCK 0.1` build and delivers the Runtime-backed
model connection test in a uniquely named installer.

## User-visible changes

- The top bar displays `APP 0.1.1`; the real installed build also displays `LOCAL JIR 0.2` and `REAL LOCAL RPC · JIR 0.2`.
- The model drawer contains “测试连接”, “测试中…”, and “重新测试” states.
- A complete saved desktop profile is sufficient; missing `OPENAI_API_KEY` and `JINGWEI_OPENAI_MODEL` are labelled optional
  and do not block testing.
- The validated non-secret profile from `com.jingweiagent.workbench.dev` is copied once when the formal application profile
  does not yet exist. The shared Windows Credential Manager entry remains the only API-key source.

## Release boundary

The installer remains an unsigned Windows Alpha. It is not a Production Release and does not claim that a commercial model
endpoint passed. Pressing “测试连接” is an explicit user action that transmits the saved API credential and bounded test prompt
to the configured endpoint.
