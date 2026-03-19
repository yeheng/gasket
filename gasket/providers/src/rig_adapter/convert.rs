//! Rig provider adapter - 公共转换工具
//!
//! 提供消息类型转换、响应构建等通用功能。

use rig::{
    completion::Usage as RigUsage,
    message::AssistantContent,
    OneOrMany,
};
use tracing::debug;

use crate::{
    ChatMessage, ChatResponse, MessageRole, ToolCall, ToolDefinition, Usage,
};

/// 将 gasket ChatMessage 转换为 rig Message
///
/// 支持所有消息角色：System, User, Assistant, Tool
pub fn to_rig_messages(messages: Vec<ChatMessage>) -> Vec<rig::message::Message> {
    messages.into_iter().map(convert_message).collect()
}

/// 转换单条 gasket ChatMessage 为 rig Message
fn convert_message(msg: ChatMessage) -> rig::message::Message {
    match msg.role {
        MessageRole::User => {
            let content = msg.content.unwrap_or_default();
            rig::message::Message::user(content)
        }
        MessageRole::Assistant => {
            if let Some(ref tool_calls) = msg.tool_calls {
                // 带工具调用的 assistant 消息
                let contents: Vec<AssistantContent> = tool_calls
                    .iter()
                    .map(|tc| {
                        AssistantContent::tool_call(
                            &tc.id,
                            &tc.function.name,
                            tc.function.arguments.clone(),
                        )
                    })
                    .collect();

                // 如果有 content，也添加文本内容
                let mut all_contents = Vec::new();
                if let Some(text) = &msg.content {
                    if !text.is_empty() {
                        all_contents.push(AssistantContent::text(text));
                    }
                }
                all_contents.extend(contents);

                rig::message::Message::Assistant {
                    id: None,
                    content: OneOrMany::many(all_contents)
                        .unwrap_or_else(|_| OneOrMany::one(AssistantContent::text(""))),
                }
            } else {
                rig::message::Message::assistant(msg.content.unwrap_or_default())
            }
        }
        MessageRole::System => {
            // rig 没有 system role，使用 preamble 替代
            // 这里我们将其转换为特殊的 user message
            // 在实际使用中，system message 应该通过 preamble 传递
            let content = format!("[System] {}", msg.content.unwrap_or_default());
            rig::message::Message::user(content)
        }
        MessageRole::Tool => {
            // 工具结果消息
            let tool_call_id = msg.tool_call_id.unwrap_or_default();
            let content = msg.content.unwrap_or_default();

            rig::message::Message::tool_result(tool_call_id, content)
        }
    }
}

/// 将 gasket ToolDefinition 转换为 rig ToolDefinition
pub fn to_rig_tool_def(def: &ToolDefinition) -> rig::completion::ToolDefinition {
    rig::completion::ToolDefinition {
        name: def.function.name.clone(),
        description: def.function.description.clone(),
        parameters: def.function.parameters.clone(),
    }
}

/// 将 rig ToolDefinition 转换为 gasket ToolDefinition
#[allow(dead_code)]
pub fn from_rig_tool_def(def: rig::completion::ToolDefinition) -> ToolDefinition {
    ToolDefinition::function(def.name, def.description, def.parameters)
}

/// 从 rig 的 AssistantContent 列表构建 ChatResponse
pub fn build_chat_response(
    contents: Vec<AssistantContent>,
    usage: Option<RigUsage>,
) -> ChatResponse {
    let mut content = None;
    let mut tool_calls = Vec::new();
    let mut reasoning_content = None;

    for item in contents {
        match item {
            AssistantContent::Text(text) => {
                content = Some(text.text);
            }
            AssistantContent::ToolCall(tc) => {
                tool_calls.push(ToolCall::new(
                    tc.id,
                    tc.function.name,
                    tc.function.arguments,
                ));
            }
            AssistantContent::Reasoning(r) => {
                reasoning_content = Some(r.display_text());
            }
            AssistantContent::Image(_) => {
                // 暂不支持 assistant 返回图片
                debug!("Ignoring image content in assistant response");
            }
        }
    }

    ChatResponse {
        content,
        tool_calls,
        reasoning_content,
        usage: usage.map(|u| Usage {
            input_tokens: u.input_tokens as usize,
            output_tokens: u.output_tokens as usize,
            total_tokens: u.total_tokens as usize,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_to_rig_messages_user() {
        let messages = vec![ChatMessage::user("Hello")];
        let rig_messages = to_rig_messages(messages);

        assert_eq!(rig_messages.len(), 1);
        match &rig_messages[0] {
            rig::message::Message::User { content } => {
                let first = content.first();
                match first {
                    rig::message::UserContent::Text(text) => assert_eq!(text.text, "Hello"),
                    _ => panic!("Expected text content"),
                }
            }
            _ => panic!("Expected user message"),
        }
    }

    #[test]
    fn test_to_rig_messages_assistant() {
        let messages = vec![ChatMessage::assistant("Hi there!")];
        let rig_messages = to_rig_messages(messages);

        assert_eq!(rig_messages.len(), 1);
        match &rig_messages[0] {
            rig::message::Message::Assistant { content, .. } => {
                let first = content.first();
                match first {
                    AssistantContent::Text(text) => assert_eq!(text.text, "Hi there!"),
                    _ => panic!("Expected text content"),
                }
            }
            _ => panic!("Expected assistant message"),
        }
    }

    #[test]
    fn test_to_rig_messages_with_tool_calls() {
        let tool_calls = vec![ToolCall::new(
            "call_123",
            "get_weather",
            json!({"location": "Beijing"}),
        )];
        let messages = vec![ChatMessage::assistant_with_tools(None, tool_calls)];
        let rig_messages = to_rig_messages(messages);

        assert_eq!(rig_messages.len(), 1);
        match &rig_messages[0] {
            rig::message::Message::Assistant { content, .. } => {
                let items: Vec<_> = content.iter().collect();
                assert!(items.iter().any(|item| matches!(item, AssistantContent::ToolCall(_))));
            }
            _ => panic!("Expected assistant message"),
        }
    }

    #[test]
    fn test_to_rig_tool_def() {
        let gasket_def = ToolDefinition::function(
            "get_weather",
            "Get weather for a location",
            json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" }
                }
            }),
        );

        let rig_def = to_rig_tool_def(&gasket_def);

        assert_eq!(rig_def.name, "get_weather");
        assert_eq!(rig_def.description, "Get weather for a location");
    }

    #[test]
    fn test_build_chat_response_text() {
        let contents = vec![AssistantContent::text("Hello, world!")];
        let response = build_chat_response(contents, None);

        assert_eq!(response.content, Some("Hello, world!".to_string()));
        assert!(response.tool_calls.is_empty());
        assert!(response.reasoning_content.is_none());
    }

    #[test]
    fn test_build_chat_response_with_tool_call() {
        let contents = vec![
            AssistantContent::text("Let me check that."),
            AssistantContent::tool_call(
                "call_123",
                "search",
                json!({"query": "rust lang"}),
            ),
        ];
        let response = build_chat_response(contents, None);

        assert_eq!(response.content, Some("Let me check that.".to_string()));
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_123");
    }

    #[test]
    fn test_build_chat_response_with_reasoning() {
        let contents = vec![
            AssistantContent::Reasoning(rig::message::Reasoning::new(
                "Thinking about the answer...",
            )),
            AssistantContent::text("The answer is 42."),
        ];
        let response = build_chat_response(contents, None);

        assert_eq!(response.content, Some("The answer is 42.".to_string()));
        assert_eq!(
            response.reasoning_content,
            Some("Thinking about the answer...".to_string())
        );
    }
}