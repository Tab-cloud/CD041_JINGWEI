# @jingwei/agent-policy

Layered project/session/agent/tool rules with deny as an absolute upper bound. Visibility is separate from authorization.
Root-only tools are stripped from specialists. Approval requests and decisions are durable before events or coroutine wakeups;
scoped approvals are limited to the same session, permission, resources, and expiry.
