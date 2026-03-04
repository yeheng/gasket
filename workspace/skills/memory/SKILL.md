---
name: memory
description: Manage long-term memory using markdown files and SQLite database
always: false
---

# Memory Management Skill

This skill provides guidance on using nanobot's hybrid long-term memory system.

## Overview

nanobot utilizes a powerful two-tier hybrid memory system combining the flexibility of Markdown and the robustness of SQLite:

1. **Markdown Files (e.g., MEMORY.md)** - High-level, semantic long-term memory for important facts, user preferences, knowledge, and system-wide configurations.
2. **SQLite Database** - Structured, reliable, and queryable chronological log of historical events, activities, and past conversations.

## When to Use Memory

Use the memory system to store and retrieve:
- Important user preferences (e.g., "User prefers Python over JavaScript")
- Key facts about projects (e.g., "Project uses PostgreSQL database")
- Recurring patterns or decisions (e.g., "Always use async/await for I/O operations")
- Chronological history of events for auditing and context resumption.

## How to Write to Memory

### Markdown Memory (MEMORY.md)

Markdown files are used for core, persistent facts and preferences. Use `write_file` or `edit_file` to add information into the workspace memory files:

```
Use write_file to create or edit:
~/.nanobot/memory/MEMORY.md (or equivalent workspace paths)

Content format:
# Long-Term Memory

## User Preferences
- Prefers dark mode in all applications
- Uses VS Code as primary editor
- Favorite language: Rust

## Project Context
- Working on nanobot project
- Migration from Python to Rust in progress
```

### SQLite History (HISTORY)

Chronological events and historical interactions are automatically or programmatically written to the SQLite database. This replaces the old `HISTORY.md` approach, providing faster queries and better scale. 

*Note: History logging is typically handled by the nanobot core systems (e.g., via the SQLite memory provider) during interactions. When operating as an agent, you usually don't need to manually write raw SQL, as the overarching agent loop and hooks will ingest the conversation history to the DB.*

## How to Read Memory

### Reading Markdown Knowledge
Use file reading tools (`read_file` or `view_file`) to check existing knowledge and preferences:

```
read_file: ~/.nanobot/memory/MEMORY.md
```

### Retrieving History from SQLite
Historical interactions are retrieved from the SQLite database. The nanobot framework handles history loading by pulling recent conversational context and past tool executions directly from the database into the current context window.

## Best Practices

1. **Be Selective for Markdown**: Only store truly important, reusable, and enduring information in `MEMORY.md`.
2. **Be Concise**: Keep markdown entries brief and easily searchable.
3. **Use Categories**: Organize `MEMORY.md` with clear architectural and contextual sections.
4. **Rely on SQLite for Timestamps**: Let the SQLite database handle chronological logging. You do not need to manually append timestamps to markdown files for every single action anymore.
5. **Review Regularly**: Periodically review and clean up `MEMORY.md` to ensure rules and context are up-to-date.

## Memory Window

The agent has a limited memory window in a single session (e.g., 50 messages). 
- Long conversations will have their older messages naturally reside in the **SQLite database** as historical context. 
- Important synthesized facts learned during the session should be explicitly saved by editing **MEMORY.md**.

## Important Notes

- Hybrid memory persists across system restarts and sessions.
- Core knowledge is stored in plain Markdown files (human-readable and easily editable).
- History is stored in SQLite for performance and structured querying.
- Use memory intelligently to avoid asking repetitive questions and to maintain context over long-running projects.
