## ADDED Requirements

### Requirement: Action Tracker

NanoBot SHALL provide an action tracker for rate limiting via sliding window.

```rust
pub struct ActionTracker {
    actions: Mutex<Vec<Instant>>,
    window_secs: u64,
}
```

#### Scenario: Action tracker initialization
- **GIVEN** a new `ActionTracker`
- **WHEN** it is created
- **THEN** the action count SHALL be zero

#### Scenario: Sliding window configuration
- **WHEN** an `ActionTracker` is created with `window_secs`
- **THEN** actions older than `window_secs` SHALL be excluded from count

---

### Requirement: Action Recording

The ActionTracker SHALL support recording actions and returning current count.

```rust
pub fn record(&self) -> usize;
```

#### Scenario: Recording increments count
- **GIVEN** an `ActionTracker` with count 0
- **WHEN** `record()` is called
- **THEN** the return value SHALL be 1
- **AND** subsequent calls SHALL return incrementing values

#### Scenario: Old actions expire
- **GIVEN** an `ActionTracker` with 1-hour window
- **AND** an action recorded 2 hours ago
- **WHEN** `record()` is called
- **THEN** the old action SHALL NOT be counted

#### Scenario: Thread-safe recording
- **GIVEN** an `ActionTracker`
- **WHEN** multiple threads call `record()` concurrently
- **THEN** all calls SHALL succeed
- **AND** the count SHALL be accurate

---

### Requirement: Rate Limit Checking

The ActionTracker SHALL support checking rate limits without recording.

```rust
pub fn is_limited(&self, max_actions: usize) -> bool;
```

#### Scenario: Under limit returns false
- **GIVEN** an `ActionTracker` with 5 actions
- **WHEN** `is_limited(10)` is called
- **THEN** it SHALL return `false`

#### Scenario: At limit returns true
- **GIVEN** an `ActionTracker` with 10 actions
- **WHEN** `is_limited(10)` is called
- **THEN** it SHALL return `true`

#### Scenario: Over limit returns true
- **GIVEN** an `ActionTracker` with 15 actions
- **WHEN** `is_limited(10)` is called
- **THEN** it SHALL return `true`

#### Scenario: Zero limit blocks all
- **GIVEN** `max_actions` is 0
- **WHEN** `is_limited(0)` is called
- **THEN** it SHALL return `true`

---

### Requirement: Rate Limiting Integration with SecurityPolicy

The SecurityPolicy SHALL use ActionTracker for rate limiting.

#### Scenario: Rate limit enforced on command execution
- **GIVEN** `max_actions_per_hour` is 5
- **AND** 5 commands have been executed in the last hour
- **WHEN** a 6th command is attempted
- **THEN** the execution SHALL be rejected
- **AND** an error message SHALL indicate rate limiting

#### Scenario: Rate limit resets after window
- **GIVEN** `max_actions_per_hour` is 5
- **AND** 5 commands were executed 2 hours ago
- **WHEN** a new command is attempted
- **THEN** the execution SHALL be allowed

#### Scenario: Security event recorded on rate limit
- **WHEN** a command is rejected due to rate limiting
- **THEN** a `SecurityEvent` SHALL be recorded
- **AND** the event SHALL indicate rate limit exceeded

---

### Requirement: Rate Limiting Configuration

Rate limiting SHALL be configurable via the security configuration.

```yaml
security:
  maxActionsPerHour: number
```

#### Scenario: Default rate limit
- **GIVEN** no `maxActionsPerHour` in configuration
- **WHEN** the configuration is loaded
- **THEN** `max_actions_per_hour` SHALL default to 20

#### Scenario: Custom rate limit
- **GIVEN** `maxActionsPerHour: 100`
- **WHEN** the configuration is loaded
- **THEN** up to 100 actions per hour SHALL be allowed

#### Scenario: Disabled rate limiting
- **GIVEN** `maxActionsPerHour: 0`
- **WHEN** the configuration is loaded
- **THEN** no actions SHALL be allowed (useful for read-only mode)
