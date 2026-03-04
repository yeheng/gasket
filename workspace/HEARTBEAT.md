---
summary: "Workspace template for HEARTBEAT.md"
read_when:
  - Bootstrapping a workspace manually
---

# HEARTBEAT.md

Heartbeat tasks are checked periodically (default: every 30 minutes).

## Format

Use `- [ ]` prefix for each task (same as GitHub-flavored markdown checkboxes):

```markdown
- [ ] Your task description here
- [ ] Another task to check
```

## Example Tasks

- [ ] Check email inbox for unread messages
- [ ] Review today's calendar events
- [ ] Check system notifications

## Notes

- Lines without `- [ ]` prefix are ignored (treated as comments)
- Keep this file small to limit token usage
- For precise timing (e.g., "9 AM every Monday"), use cron jobs instead
