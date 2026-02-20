## ADDED Requirements

### Requirement: Security Policy Module

NanoBot SHALL provide a comprehensive security policy module for controlling agent autonomy and command execution.

- The module SHALL be located at `nanobot-core/src/security/`
- The module SHALL export `SecurityPolicy`, `AutonomyLevel`, and `CommandRisk` types
- Default configuration SHALL be secure-by-default

#### Scenario: Default security policy is restrictive
- **GIVEN** a new SecurityPolicy instance
- **WHEN** no configuration is provided
- **THEN** `autonomy` SHALL be `Supervised`
- **AND** `workspace_only` SHALL be `true`
- **AND** `block_high_risk_commands` SHALL be `true`
- **AND** `require_approval_for_medium_risk` SHALL be `true`

#### Scenario: Autonomy levels control agent behavior
- **WHEN** autonomy level is `ReadOnly`
- **THEN** no shell commands SHALL be allowed
- **WHEN** autonomy level is `Supervised`
- **THEN** risky commands SHALL require approval
- **WHEN** autonomy level is `Full`
- **THEN** commands within policy SHALL be allowed automatically

---

### Requirement: Command Risk Classification

NanoBot SHALL classify shell commands into risk levels for fine-grained security control.

#### Scenario: High-risk commands are identified
- **WHEN** a command contains `rm`, `sudo`, `su`, `curl`, `wget`, `ssh`, `scp`, `chmod`, `chown`, `dd`, `shutdown`, `reboot`, `mount`, `umount`
- **THEN** it SHALL be classified as `CommandRisk::High`

#### Scenario: Medium-risk commands are identified
- **WHEN** a command is `git` with subcommands `commit`, `push`, `reset`, `clean`, `rebase`, `merge`
- **OR** a command is `npm`/`yarn`/`pnpm` with subcommands `install`, `add`, `remove`, `publish`
- **OR** a command is `cargo` with subcommands `add`, `remove`, `install`, `publish`
- **OR** a command is `touch`, `mkdir`, `mv`, `cp`, `ln`
- **THEN** it SHALL be classified as `CommandRisk::Medium`

#### Scenario: Low-risk commands are identified
- **WHEN** a command is `ls`, `cat`, `grep`, `find`, `head`, `tail`, `echo`, `pwd`, `wc`
- **THEN** it SHALL be classified as `CommandRisk::Low`

#### Scenario: Command chains are analyzed
- **WHEN** a command contains `&&`, `||`, `;`, or `|`
- **THEN** each segment SHALL be classified separately
- **AND** the highest risk level SHALL be returned

---

### Requirement: Command Injection Prevention

NanoBot SHALL prevent common command injection attacks.

#### Scenario: Backtick injection is blocked
- **WHEN** a command contains backticks (`` ` ``)
- **THEN** the command SHALL be rejected
- **AND** `is_command_allowed()` SHALL return `false`

#### Scenario: Dollar-parenthesis injection is blocked
- **WHEN** a command contains `$(` or `${`
- **THEN** the command SHALL be rejected

#### Scenario: Output redirection is blocked
- **WHEN** a command contains `>` or `>>`
- **THEN** the command SHALL be rejected

#### Scenario: Semicolon injection is blocked
- **WHEN** a command contains `;` followed by another command
- **AND** the second command is not in the allowlist
- **THEN** the command SHALL be rejected

---

### Requirement: Path Access Control

NanoBot SHALL enforce path access restrictions to prevent unauthorized file access.

#### Scenario: Path traversal is blocked
- **WHEN** a path contains `..` as a path component
- **THEN** `is_path_allowed()` SHALL return `false`

#### Scenario: Null byte injection is blocked
- **WHEN** a path contains a null byte (`\0`)
- **THEN** `is_path_allowed()` SHALL return `false`

#### Scenario: URL-encoded traversal is blocked
- **WHEN** a path contains `..%2f` or `%2f..`
- **THEN** `is_path_allowed()` SHALL return `false`

#### Scenario: Workspace-only mode restricts paths
- **GIVEN** `workspace_only` is `true`
- **WHEN** an absolute path is provided
- **THEN** `is_path_allowed()` SHALL return `false`

#### Scenario: Forbidden paths are blocked
- **WHEN** a path starts with a forbidden path (e.g., `/etc`, `~/.ssh`)
- **THEN** `is_path_allowed()` SHALL return `false`

#### Scenario: Resolved paths must be in workspace
- **WHEN** `is_resolved_path_allowed()` is called
- **THEN** the path SHALL be canonicalized
- **AND** the resolved path MUST start with the workspace directory

---

### Requirement: Security Policy Configuration

NanoBot SHALL support security policy configuration via YAML.

```yaml
security:
  autonomyLevel: supervised | readonly | full
  workspaceOnly: boolean
  allowedCommands: [string]
  forbiddenPaths: [string]
  maxActionsPerHour: number
  requireApprovalForMediumRisk: boolean
  blockHighRiskCommands: boolean
```

#### Scenario: Configuration is parsed correctly
- **GIVEN** a valid YAML configuration with `security` section
- **WHEN** the configuration is loaded
- **THEN** all security settings SHALL be correctly parsed
- **AND** missing fields SHALL use safe defaults

#### Scenario: Autonomy level serialization
- **WHEN** autonomy level is `readonly`
- **THEN** it SHALL serialize to `"readonly"` in YAML
- **WHEN** autonomy level is `supervised`
- **THEN** it SHALL serialize to `"supervised"` in YAML

---

### Requirement: Security Policy Integration with ExecTool

The ExecTool SHALL enforce the security policy for all command executions.

#### Scenario: Disabled tool rejects all commands
- **WHEN** `enabled` is `false`
- **THEN** all commands SHALL be rejected regardless of policy

#### Scenario: High-risk commands are blocked by default
- **GIVEN** `block_high_risk_commands` is `true`
- **WHEN** a high-risk command is executed
- **THEN** the execution SHALL fail with a security error

#### Scenario: Medium-risk commands require approval
- **GIVEN** `require_approval_for_medium_risk` is `true`
- **WHEN** a medium-risk command is executed without approval
- **THEN** the execution SHALL fail with an approval required error

#### Scenario: Rate limiting is enforced
- **GIVEN** `max_actions_per_hour` is set
- **WHEN** the action count exceeds the limit
- **THEN** subsequent executions SHALL fail with a rate limit error
