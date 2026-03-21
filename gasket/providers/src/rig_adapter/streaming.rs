//! Rig 流式响应转换
//!
//! 将 rig 的流式响应转换为 gasket 的 ChatStream。
//!
//! 注意：这个模块处理的是单次请求的流式响应，不处理多轮对话循环。
//! 多轮循环由 Gasket 的 AgentExecutor 控制。

use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::{StreamedAssistantContent, StreamingCompletionResponse};

use crate::{ChatStream, ChatStreamChunk, ChatStreamDelta, FinishReason, ToolCallDelta, Usage};

/// 将 rig 的 StreamingCompletionResponse 转换为 gasket 的 ChatStream
///
/// 这个函数处理单次 LLM 请求的流式响应，包括：
/// - 文本内容 (Text)
/// - 推理/思考内容 (Reasoning)
/// - 工具调用 (ToolCall/ToolCallDelta)
/// - 最终响应和 token 使用统计
///
/// 适用于使用 `CompletionModel.stream()` API 的场景。
///
/// 别名：`convert_rig_multi_turn_stream` (向后兼容)
pub fn convert_rig_stream<R>(mut response: StreamingCompletionResponse<R>) -> ChatStream
where
    R: Clone + Unpin + GetTokenUsage + Send + 'static,
{
    use async_stream::stream;

    // StreamingCompletionResponse 本身实现了 Stream trait
    let s = stream! {
        while let Some(chunk_result) = response.next().await {
            match chunk_result {
                Ok(item) => {
                    match item {
                        StreamedAssistantContent::Text(text) => {
                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta {
                                    content: Some(text.text),
                                    reasoning_content: None,
                                    tool_calls: vec![],
                                },
                                finish_reason: None,
                                usage: None,
                            });
                        }
                        StreamedAssistantContent::Reasoning(reasoning) => {
                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta {
                                    content: None,
                                    reasoning_content: Some(reasoning.display_text()),
                                    tool_calls: vec![],
                                },
                                finish_reason: None,
                                usage: None,
                            });
                        }
                        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta {
                                    content: None,
                                    reasoning_content: Some(reasoning),
                                    tool_calls: vec![],
                                },
                                finish_reason: None,
                                usage: None,
                            });
                        }
                        StreamedAssistantContent::ToolCall { tool_call, .. } => {
                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta {
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: vec![ToolCallDelta {
                                        index: 0,
                                        id: Some(tool_call.id),
                                        function_name: Some(tool_call.function.name),
                                        function_arguments: Some(
                                            serde_json::to_string(&tool_call.function.arguments)
                                                .unwrap_or_default()
                                        ),
                                    }],
                                },
                                finish_reason: Some(FinishReason::ToolCalls),
                                usage: None,
                            });
                        }
                        StreamedAssistantContent::ToolCallDelta { id, content, .. } => {
                            let (function_name, function_arguments) = match content {
                                rig::streaming::ToolCallDeltaContent::Name(name) => (Some(name), None),
                                rig::streaming::ToolCallDeltaContent::Delta(args) => (None, Some(args)),
                            };

                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta {
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: vec![ToolCallDelta {
                                        index: 0,
                                        id: Some(id),
                                        function_name,
                                        function_arguments,
                                    }],
                                },
                                finish_reason: None,
                                usage: None,
                            });
                        }
                        StreamedAssistantContent::Final(final_response) => {
                            let usage = final_response.token_usage().map(|u| Usage {
                                input_tokens: u.input_tokens as usize,
                                output_tokens: u.output_tokens as usize,
                                total_tokens: u.total_tokens as usize,
                            });

                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta::default(),
                                finish_reason: Some(FinishReason::Stop),
                                usage,
                            });
                        }
                    }
                }
                Err(e) => {
                    yield Err(anyhow::anyhow!("Stream error: {}", e));
                    break;
                }
            }
        }
    };

    Box::pin(s)
}

/// 将 rig 的 MultiTurnStream (StreamingResult) 转换为 gasket 的 ChatStream
///
/// 这个函数处理来自 `agent.stream_prompt()` 的多轮流式响应。
/// Rig 的 agent 会自动处理工具调用循环，但我们只需要提取
/// 单次请求的流式内容。
///
/// 适用于使用 `agent.stream_prompt()` API 的场景（旧 API）。
pub fn convert_rig_multi_turn_stream<R>(mut stream: rig::agent::StreamingResult<R>) -> ChatStream
where
    R: Clone + Unpin + GetTokenUsage + Send + 'static,
{
    use async_stream::stream;
    use rig::agent::MultiTurnStreamItem;
    use rig::streaming::StreamedAssistantContent;

    let s = stream! {
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(item) => {
                    match item {
                        MultiTurnStreamItem::StreamAssistantItem(stream_item) => {
                            match stream_item {
                                StreamedAssistantContent::Text(text) => {
                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta {
                                            content: Some(text.text),
                                            reasoning_content: None,
                                            tool_calls: vec![],
                                        },
                                        finish_reason: None,
                                        usage: None,
                                    });
                                }
                                StreamedAssistantContent::Reasoning(reasoning) => {
                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta {
                                            content: None,
                                            reasoning_content: Some(reasoning.display_text()),
                                            tool_calls: vec![],
                                        },
                                        finish_reason: None,
                                        usage: None,
                                    });
                                }
                                StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta {
                                            content: None,
                                            reasoning_content: Some(reasoning),
                                            tool_calls: vec![],
                                        },
                                        finish_reason: None,
                                        usage: None,
                                    });
                                }
                                StreamedAssistantContent::ToolCall { tool_call, .. } => {
                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta {
                                            content: None,
                                            reasoning_content: None,
                                            tool_calls: vec![ToolCallDelta {
                                                index: 0,
                                                id: Some(tool_call.id),
                                                function_name: Some(tool_call.function.name),
                                                function_arguments: Some(
                                                    serde_json::to_string(&tool_call.function.arguments)
                                                        .unwrap_or_default()
                                                ),
                                            }],
                                        },
                                        finish_reason: Some(FinishReason::ToolCalls),
                                        usage: None,
                                    });
                                }
                                StreamedAssistantContent::ToolCallDelta { id, content, .. } => {
                                    let (function_name, function_arguments) = match content {
                                        rig::streaming::ToolCallDeltaContent::Name(name) => (Some(name), None),
                                        rig::streaming::ToolCallDeltaContent::Delta(args) => (None, Some(args)),
                                    };

                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta {
                                            content: None,
                                            reasoning_content: None,
                                            tool_calls: vec![ToolCallDelta {
                                                index: 0,
                                                id: Some(id),
                                                function_name,
                                                function_arguments,
                                            }],
                                        },
                                        finish_reason: None,
                                        usage: None,
                                    });
                                }
                                StreamedAssistantContent::Final(final_response) => {
                                    let usage = final_response.token_usage().map(|u| Usage {
                                        input_tokens: u.input_tokens as usize,
                                        output_tokens: u.output_tokens as usize,
                                        total_tokens: u.total_tokens as usize,
                                    });

                                    yield Ok(ChatStreamChunk {
                                        delta: ChatStreamDelta::default(),
                                        finish_reason: Some(FinishReason::Stop),
                                        usage,
                                    });
                                }
                            }
                        }
                        MultiTurnStreamItem::StreamUserItem(_) => {
                            // 工具结果消息，由 Rig agent 自动处理
                            // 我们不需要转发给 Gasket 的 AgentExecutor
                        }
                        MultiTurnStreamItem::FinalResponse(res) => {
                            // 最终响应
                            let rig_usage = res.usage();
                            let usage = Usage {
                                input_tokens: rig_usage.input_tokens as usize,
                                output_tokens: rig_usage.output_tokens as usize,
                                total_tokens: rig_usage.total_tokens as usize,
                            };

                            yield Ok(ChatStreamChunk {
                                delta: ChatStreamDelta::default(),
                                finish_reason: Some(FinishReason::Stop),
                                usage: Some(usage),
                            });
                        }
                        // 处理其他未知类型（non-exhaustive enum）
                        _ => {}
                    }
                }
                Err(e) => {
                    yield Err(anyhow::anyhow!("Stream error: {}", e));
                    break;
                }
            }
        }
    };

    Box::pin(s)
}
