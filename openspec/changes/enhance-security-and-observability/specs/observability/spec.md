## ADDED Requirements

### Requirement: Observability Module

NanoBot SHALL provide an observability module for monitoring agent behavior.

- The module SHALL be located at `nanobot-core/src/observability/`
- The module SHALL export an `Observer` trait and event types
- The default observer SHALL have zero runtime overhead

#### Scenario: Module structure
- **GIVEN** the observability module
- **WHEN** the module is imported
- **THEN** `Observer`, `ObserverEvent`, and `MessageDirection` SHALL be available

---

### Requirement: Observer Trait

NanoBot SHALL define an `Observer` trait for recording events.

```rust
pub trait Observer: Send + Sync {
    fn record(&self, event: &ObserverEvent);
}
```

#### Scenario: Observer is thread-safe
- **GIVEN** an `Observer` implementation
- **THEN** it SHALL implement `Send + Sync`
- **AND** it SHALL be usable across async boundaries

#### Scenario: Observer records events
- **GIVEN** an `Observer` instance
- **WHEN** `record(event)` is called
- **THEN** the event SHALL be processed according to the implementation

---

### Requirement: Observer Events

NanoBot SHALL define standard event types for observability.

```rust
pub enum ObserverEvent {
    AgentStart { provider: String, model: String },
    AgentEnd { duration: Duration, tokens_used: Option<u64> },
    ToolCall { tool: String, duration: Duration, success: bool },
    ChannelMessage { channel: String, direction: MessageDirection },
    SecurityEvent { event_type: String, details: String },
    Error { context: String, error: String },
}
```

#### Scenario: Agent lifecycle events
- **WHEN** an agent session starts
- **THEN** an `AgentStart` event SHALL be recorded
- **WHEN** an agent session ends
- **THEN** an `AgentEnd` event SHALL be recorded with duration

#### Scenario: Tool execution events
- **WHEN** a tool is invoked
- **THEN** a `ToolCall` event SHALL be recorded after execution
- **AND** the event SHALL include tool name, duration, and success status

#### Scenario: Channel message events
- **WHEN** a message is received from or sent to a channel
- **THEN** a `ChannelMessage` event SHALL be recorded

#### Scenario: Security events
- **WHEN** a security policy violation occurs
- **THEN** a `SecurityEvent` SHALL be recorded
- **AND** the event SHALL include event type and details

---

### Requirement: NoopObserver Implementation

NanoBot SHALL provide a no-operation observer for zero-overhead scenarios.

#### Scenario: NoopObserver has no side effects
- **GIVEN** a `NoopObserver` instance
- **WHEN** `record(event)` is called
- **THEN** no action SHALL be taken
- **AND** no memory SHALL be allocated

#### Scenario: NoopObserver is the default
- **GIVEN** no observability configuration
- **WHEN** an observer is created
- **THEN** a `NoopObserver` SHALL be used

---

### Requirement: LogObserver Implementation

NanoBot SHALL provide a logging observer for basic observability.

#### Scenario: LogObserver logs events
- **GIVEN** a `LogObserver` with level `INFO`
- **WHEN** `record(event)` is called
- **THEN** the event SHALL be logged via `tracing`
- **AND** the log level SHALL match the configured level

#### Scenario: LogObserver formats events readably
- **GIVEN** a `LogObserver`
- **WHEN** a `ToolCall` event is recorded
- **THEN** the log SHALL include tool name, duration, and success status

---

### Requirement: Observer Factory

NanoBot SHALL provide a factory function for creating observers.

```rust
pub fn create_observer(config: &ObservabilityConfig) -> Box<dyn Observer>;
```

#### Scenario: Factory creates NoopObserver
- **GIVEN** `observability.kind` is `"noop"` or not specified
- **WHEN** `create_observer()` is called
- **THEN** a `NoopObserver` SHALL be returned

#### Scenario: Factory creates LogObserver
- **GIVEN** `observability.kind` is `"log"`
- **WHEN** `create_observer()` is called
- **THEN** a `LogObserver` SHALL be returned
- **AND** the log level SHALL match `observability.level`

---

### Requirement: Observability Configuration

NanoBot SHALL support observability configuration via YAML.

```yaml
observability:
  kind: noop | log
  level: trace | debug | info | warn | error
```

#### Scenario: Default configuration is noop
- **GIVEN** no `observability` section in config
- **WHEN** the configuration is loaded
- **THEN** `kind` SHALL default to `"noop"`

#### Scenario: Log level defaults to info
- **GIVEN** `observability.kind` is `"log"`
- **AND** `observability.level` is not specified
- **THEN** the level SHALL default to `"info"`

---

### Requirement: Observer Integration in Agent Loop

The agent loop SHALL record events via the configured observer.

#### Scenario: Agent start is recorded
- **WHEN** the agent loop begins execution
- **THEN** an `AgentStart` event SHALL be recorded
- **AND** the event SHALL include provider name and model

#### Scenario: Agent end is recorded
- **WHEN** the agent loop completes
- **THEN** an `AgentEnd` event SHALL be recorded
- **AND** the event SHALL include total duration

#### Scenario: Tool calls are recorded
- **WHEN** a tool is executed by the agent
- **THEN** a `ToolCall` event SHALL be recorded
- **AND** the event SHALL include execution duration and success status

#### Scenario: Security violations are recorded
- **WHEN** a command is blocked by security policy
- **THEN** a `SecurityEvent` SHALL be recorded
- **AND** the event SHALL include the blocked command and reason
