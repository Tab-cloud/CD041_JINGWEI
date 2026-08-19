# @jingwei/agent-events

Append-only durable events with aggregate-local monotonic sequence numbers. `DurableEventBus.events()` performs history
replay before live waiting and closes the history/live race. SQLite migrations fail closed when the database schema is newer
or invalid; projections reject sequence gaps.
