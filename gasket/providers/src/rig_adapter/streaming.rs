//! Rig 流式响应转换
//!
//! 将 rig 的 StreamingResult 转换为 gasket 的 ChatStream。

use futures::StreamExt;
use rig::{
    completion::GetTokenUsage,
    streaming::StreamedAssistantContent,
};

use crate::{
    ChatStream, ChatStreamChunk, ChatStreamDelta, FinishReason, ToolCallDelta, Usage,
};

/// 将 rig 的 StreamingResult (MultiTurnStreamItem) 转换为 gasket 的 ChatStream
pub fn convert_rig_multi_turn_stream<R>(
    mut stream: rig::agent::StreamingResult<R>,
) -> ChatStream
where
    R: Clone + Unpin + GetTokenUsage + Send + 'static,
{
    use async_stream::stream;
    use rig::agent::MultiTurnStreamItem;

    let s = stream! {
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(MultiTurnStreamItem::StreamAssistantItem(item)) => {
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
                Ok(MultiTurnStreamItem::StreamUserItem(_)) => {
                    // 工具结果消息，忽略
                }
                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                    // 最终响应
                    let usage = Usage {
                        input_tokens: res.usage().input_tokens as usize,
                        output_tokens: res.usage().output_tokens as usize,
                        total_tokens: res.usage().total_tokens as usize,
                    };

                    yield Ok(ChatStreamChunk {
                        delta: ChatStreamDelta::default(),
                        finish_reason: Some(FinishReason::Stop),
                        usage: Some(usage),
                    });
                }
                Ok(_) => {
                    // 处理其他未知类型（non-exhaustive enum）
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