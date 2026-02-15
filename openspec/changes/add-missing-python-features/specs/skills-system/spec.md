## ADDED Requirements

### Requirement: Skills Loader

The system SHALL provide a skills loader that can dynamically load skill modules from the filesystem.

#### Scenario: Load skill with YAML frontmatter

- **WHEN** a skill file exists at `~/.nanobot/skills/my-skill.md` with valid YAML frontmatter
- **THEN** the skill loader SHALL parse the YAML metadata
- **AND** extract the Markdown content
- **AND** create a Skill object with metadata and content

#### Scenario: Load built-in skills

- **WHEN** the system initializes
- **THEN** the skill loader SHALL load all built-in skills from the internal skills directory
- **AND** register them in the Skills Registry

#### Scenario: Handle malformed skill file

- **WHEN** a skill file has invalid YAML frontmatter
- **THEN** the skill loader SHALL log an error
- **AND** skip the malformed skill
- **AND** continue loading other skills

### Requirement: Skill Metadata Validation

The system SHALL validate skill dependencies before marking a skill as available.

#### Scenario: Check binary dependencies

- **WHEN** a skill declares `bins: ["gh", "git"]` in metadata
- **THEN** the system SHALL verify each binary is in PATH
- **AND** mark the skill as unavailable if any binary is missing

#### Scenario: Check environment variable dependencies

- **WHEN** a skill declares `env_vars: ["GITHUB_TOKEN"]` in metadata
- **THEN** the system SHALL verify the environment variable exists
- **AND** mark the skill as unavailable if any variable is missing

#### Scenario: Skill with no dependencies

- **WHEN** a skill has no `bins` or `env_vars` declared
- **THEN** the system SHALL mark the skill as available

### Requirement: Progressive Skill Loading

The system SHALL support progressive skill loading to optimize context window usage.

#### Scenario: Load always-on skill

- **WHEN** a skill has `always: true` in metadata
- **THEN** the system SHALL load the complete skill content into context
- **AND** include it in every agent request

#### Scenario: Load on-demand skill

- **WHEN** a skill has `always: false` or no `always` field
- **THEN** the system SHALL only load skill metadata (name + description)
- **AND** make the skill file path available for `read_file` tool
- **AND** the agent can load full content when needed

#### Scenario: Skill summary generation

- **WHEN** building agent context
- **THEN** the system SHALL generate a skill summary section
- **AND** list all available skills with name and description
- **AND** mark unavailable skills with reason

### Requirement: Skills Registry

The system SHALL provide a registry to manage all loaded skills.

#### Scenario: Register skill

- **WHEN** a skill is successfully loaded
- **THEN** the system SHALL add it to the Skills Registry
- **AND** make it queryable by name

#### Scenario: List available skills

- **WHEN** requested to list skills
- **THEN** the system SHALL return all registered skills
- **AND** include availability status for each

#### Scenario: Filter unavailable skills

- **WHEN** building agent context
- **THEN** the system SHALL filter out unavailable skills
- **AND** log warnings for unavailable skills with reasons

### Requirement: Built-in Skills

The system SHALL include the following built-in skills:

#### Scenario: Memory skill

- **WHEN** the system loads built-in skills
- **THEN** the `memory` skill SHALL be available
- **AND** provide guidance on using long-term memory (MEMORY.md)

#### Scenario: Cron skill

- **WHEN** the system loads built-in skills
- **THEN** the `cron` skill SHALL be available
- **AND** provide guidance on managing scheduled tasks

#### Scenario: Summarize skill

- **WHEN** the system loads built-in skills
- **THEN** the `summarize` skill SHALL be available
- **AND** provide guidance on content summarization

#### Scenario: GitHub skill

- **WHEN** the system loads built-in skills
- **THEN** the `github` skill SHALL be available
- **AND** require `gh` binary in PATH
- **AND** provide guidance on GitHub operations

#### Scenario: Skill-creator skill

- **WHEN** the system loads built-in skills
- **THEN** the `skill-creator` skill SHALL be available
- **AND** provide template and guidance for creating new skills

#### Scenario: Tmux skill

- **WHEN** the system loads built-in skills
- **THEN** the `tmux` skill SHALL be available
- **AND** require `tmux` binary in PATH
- **AND** provide guidance on terminal multiplexer operations

#### Scenario: Weather skill

- **WHEN** the system loads built-in skills
- **THEN** the `weather` skill SHALL be available
- **AND** provide guidance on weather queries

### Requirement: Skill File Format

The system SHALL support skill files with YAML frontmatter format.

#### Scenario: Parse valid skill file

- **WHEN** parsing a skill file with format:
  ```markdown
  ---
  name: my-skill
  description: My skill description
  always: false
  bins: ["git"]
  env_vars: ["GITHUB_TOKEN"]
  ---

  # My Skill

  Skill content here...
  ```
- **THEN** the system SHALL extract:
  - name: "my-skill"
  - description: "My skill description"
  - always: false
  - bins: ["git"]
  - env_vars: ["GITHUB_TOKEN"]
  - content: "# My Skill\n\nSkill content here..."

#### Scenario: Handle missing optional fields

- **WHEN** a skill file omits optional fields (always, bins, env_vars)
- **THEN** the system SHALL use default values:
  - always: false
  - bins: []
  - env_vars: []

### Requirement: User Skills Directory

The system SHALL support user-defined skills in the workspace.

#### Scenario: Load user skill

- **WHEN** a file exists at `~/.nanobot/skills/custom.md`
- **THEN** the system SHALL load it as a user skill
- **AND** register it alongside built-in skills

#### Scenario: Override built-in skill

- **WHEN** a user skill has the same name as a built-in skill
- **THEN** the user skill SHALL override the built-in skill
- **AND** log a warning about the override

### Requirement: Skills in Agent Context

The system SHALL integrate loaded skills into the agent context.

#### Scenario: Include skill summary in system prompt

- **WHEN** building the system prompt for agent
- **THEN** the system SHALL include a "Available Skills" section
- **AND** list all available skills with descriptions
- **AND** provide file paths for on-demand loading

#### Scenario: Skill file accessibility

- **WHEN** an on-demand skill is listed in context
- **THEN** the skill file path SHALL be accessible via `read_file` tool
- **AND** the agent can read the full skill content when needed
