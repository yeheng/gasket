## 1. Core Types and Trait Extension

- [ ] 1.1 Add `ChatStreamChunk` and `ChatStreamDelta` types in `nanobot-core/src/llm/types.rs`
- [ ] 1.2 Add `ToolCallDelta` type for incremental tool call data
- [ ] 1.3 Add `FinishReason` enum for stream completion
- [ ] 1.4 Add `chat_stream()` method to `LlmProvider` trait with default fallback implementation
- [ ] 1.5 Add `futures` dependency to `nanobot-core/Cargo.toml`

## 2. OpenAI-Compatible Streaming Implementation

- [ ] 2.1 Add SSE parsing utilities in `nanobot-core/src/llm/streaming.rs`
- [ ] 2.2 Implement `chat_stream()` for `OpenAICompatibleProvider`
- [ ] 2.3 Handle OpenAI streaming response format (data: {...} chunks)
- [ ] 2.4 Parse `delta` objects from SSE chunks
- [ ] 2.5 Handle `[DONE]` marker for stream termination
- [ ] 2.6 Add streaming tests for OpenAI-compatible provider

## 3. Gemini Streaming Implementation

- [ ] 3.1 Add streaming endpoint support to `GeminiProvider`
- [ ] 3.2 Implement Gemini-specific streaming format parsing
- [ ] 3.3 Convert Gemini chunks to `ChatStreamChunk` format
- [ ] 3.4 Add streaming tests for Gemini provider

## 4. Agent Loop Integration

- [ ] 4.1 Add streaming mode flag to `AgentLoop` configuration
- [ ] 4.2 Implement `run_agent_loop_streaming()` method
- [ ] 4.3 Add chunk accumulation logic for complete response reconstruction
- [ ] 4.4 Implement progressive tool call accumulation
- [ ] 4.5 Add real-time output display during streaming
- [ ] 4.6 Handle streaming errors with partial response recovery

## 5. CLI Integration

- [ ] 5.1 Add `--stream` flag to CLI arguments (default: true)
- [ ] 5.2 Add `--no-stream` flag to disable streaming
- [ ] 5.3 Wire streaming flag to agent loop configuration
- [ ] 5.4 Update CLI output to display streaming chunks progressively

## 6. Middleware Updates

- [ ] 6.1 Update `LoggingProvider` to support `chat_stream()` logging
- [ ] 6.2 Update `MetricsProvider` to track streaming metrics
- [ ] 6.3 Ensure `RateLimitProvider` works with streaming requests
- [ ] 6.4 Update `ProviderBuilder` to preserve streaming support

## 7. Testing and Documentation

- [ ] 7.1 Add unit tests for `ChatStreamChunk` parsing
- [ ] 7.2 Add integration tests for streaming with mock server
- [ ] 7.3 Add e2e test for streaming CLI usage
- [ ] 7.4 Update README with streaming feature documentation

---

**Dependencies:**
- Task 1.x (types) must complete before 2.x and 3.x
- Tasks 2.x and 3.x (providers) can run in parallel
- Task 4.x (agent loop) depends on 1.x, 2.x, 3.x
- Task 5.x (CLI) depends on 4.x
- Task 6.x (middleware) can run in parallel with 2.x and 3.x
- Task 7.x (testing) depends on all previous tasks
