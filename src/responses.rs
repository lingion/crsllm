//! OpenAI Responses API (`POST /v1/responses`) support.
//!
//! This module implements the full Responses API protocol:
//! - Request parsing (input as string or array, tools, instructions, etc.)
//! - Translation to/from the internal Anthropic format
//! - Non-streaming response formatting
//! - Streaming event generation (response.created → response.completed)
//!
//! Reference: <https://platform.openai.com/docs/api-reference/responses>

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::translator::{
    AnthropicContent, AnthropicContentBlock, AnthropicMessage, AnthropicRequest,
    AnthropicResponse, AnthropicResponseContent,
};

// ─── Request types ───────────────────────────────────────────────────────────

/// Top-level request body for `POST /v1/responses`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    pub input: ResponsesInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Vec<ResponsesTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
}

/// The `input` field can be a plain string or an array of input items.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesInput {
    Text(String),
    Items(Vec<ResponsesInputItem>),
}

/// An item in the `input` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesInputItem {
    Message {
        #[serde(default)]
        role: String,
        #[serde(default)]
        content: ResponsesInputContent,
    },
    FunctionCall {
        #[serde(default)]
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
    ItemReference {
        id: String,
    },
    #[serde(other)]
    Unknown,
}

/// Content of an input message — string or array of content parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesInputContent {
    Text(String),
    Parts(Vec<ResponsesInputPart>),
}

impl Default for ResponsesInputContent {
    fn default() -> Self {
        ResponsesInputContent::Text(String::new())
    }
}

/// A content part within an input message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesInputPart {
    InputText {
        text: String,
    },
    InputImage {
        #[serde(default)]
        image_url: Option<Value>,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        file_id: Option<String>,
    },
    InputFile {
        #[serde(default)]
        file: Option<Value>,
        #[serde(default)]
        file_data: Option<String>,
        #[serde(default)]
        filename: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// A tool definition in the Responses API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesTool {
    Function {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        parameters: Option<Value>,
        #[serde(default)]
        strict: Option<bool>,
    },
    WebSearchPreview {
        #[serde(default)]
        search_context_size: Option<String>,
        #[serde(default)]
        user_location: Option<Value>,
    },
    CodeInterpreter {
        #[serde(default)]
        container: Option<Value>,
    },
    FileSearch {
        #[serde(default)]
        vector_store_ids: Option<Vec<String>>,
    },
    ComputerUsePreview {
        #[serde(default)]
        display_width: Option<u32>,
        #[serde(default)]
        display_height: Option<u32>,
        #[serde(default)]
        environment: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

// ─── Response types ──────────────────────────────────────────────────────────

/// Top-level response body for `POST /v1/responses` (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created_at: u64,
    pub model: String,
    pub output: Vec<ResponsesOutputItem>,
    pub usage: ResponsesUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// An output item in the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputItem {
    Message {
        id: String,
        role: String,
        content: Vec<ResponsesOutputContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    Reasoning {
        id: String,
        #[serde(default)]
        content: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
}

/// A content part in an output message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputContent {
    OutputText {
        text: String,
        #[serde(default)]
        annotations: Vec<Value>,
    },
}

/// Token usage in the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<Value>,
}

// ─── Translation: Responses API → Anthropic internal ─────────────────────────

/// Convert a Responses API request to the internal Anthropic format.
pub fn to_anthropic(req: &ResponsesRequest) -> AnthropicRequest {
    let mut messages = Vec::new();
    let mut system_parts: Vec<String> = Vec::new();
    let mut tools = Vec::new();

    // Instructions become the system prompt (prepended to any system messages).
    if let Some(instructions) = &req.instructions {
        if !instructions.is_empty() {
            system_parts.push(instructions.clone());
        }
    }

    // Parse input items into messages.
    match &req.input {
        ResponsesInput::Text(text) => {
            messages.push(AnthropicMessage {
                role: "user".to_string(),
                content: Some(AnthropicContent::Text(text.clone())),
                name: None,
            });
        }
        ResponsesInput::Items(items) => {
            for item in items {
                match item {
                    ResponsesInputItem::Message { role, content } => {
                        let role_lower = role.to_lowercase();
                        // "system" and "developer" roles map to system prompt.
                        if role_lower == "system" || role_lower == "developer" {
                            let text = input_content_to_text(content);
                            if !text.is_empty() {
                                system_parts.push(text);
                            }
                            continue;
                        }

                        let anthropic_content = input_content_to_anthropic(content);
                        messages.push(AnthropicMessage {
                            role: if role_lower == "developer" {
                                "system".to_string()
                            } else {
                                role_lower
                            },
                            content: Some(anthropic_content),
                            name: None,
                        });
                    }
                    ResponsesInputItem::FunctionCall { call_id, name, arguments, .. } => {
                        // A previous assistant function call — represent as assistant message with tool_use.
                        let input: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
                        messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: Some(AnthropicContent::Blocks(vec![
                                AnthropicContentBlock::ToolUse {
                                    id: call_id.clone(),
                                    name: name.clone(),
                                    input,
                                },
                            ])),
                            name: None,
                        });
                    }
                    ResponsesInputItem::FunctionCallOutput { call_id, output } => {
                        // Tool result from the user.
                        messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: Some(AnthropicContent::Blocks(vec![
                                AnthropicContentBlock::ToolResult {
                                    tool_use_id: call_id.clone(),
                                    content: crate::translator::ToolResultContent::Text(
                                        output.clone(),
                                    ),
                                    error: None,
                                },
                            ])),
                            name: None,
                        });
                    }
                    ResponsesInputItem::ItemReference { .. } | ResponsesInputItem::Unknown => {
                        // Skip unsupported item types.
                    }
                }
            }
        }
    }

    // Convert tools.
    for tool in &req.tools {
        if let ResponsesTool::Function {
            name,
            description,
            parameters,
            ..
        } = tool
        {
            tools.push(crate::translator::AnthropicTool {
                name: name.clone(),
                description: description.clone(),
                input_schema: parameters.clone().unwrap_or(json!({"type": "object", "properties": {}})),
                caching: None,
            });
        }
    }

    // Build tool_choice.
    let tool_choice = req.tool_choice.as_ref().map(|tc| translate_tool_choice(tc));

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    AnthropicRequest {
        model: req.model.clone(),
        messages,
        system,
        max_tokens: req.max_output_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        top_k: None,
        tools,
        tool_choice,
        stop_sequences: vec![],
    }
}

/// Extract plain text from an input content field.
fn input_content_to_text(content: &ResponsesInputContent) -> String {
    match content {
        ResponsesInputContent::Text(s) => s.clone(),
        ResponsesInputContent::Parts(parts) => {
            parts
                .iter()
                .filter_map(|p| match p {
                    ResponsesInputPart::InputText { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// Convert input content to Anthropic content.
fn input_content_to_anthropic(content: &ResponsesInputContent) -> AnthropicContent {
    match content {
        ResponsesInputContent::Text(s) => AnthropicContent::Text(s.clone()),
        ResponsesInputContent::Parts(parts) => {
            let mut blocks = Vec::new();
            for part in parts {
                match part {
                    ResponsesInputPart::InputText { text } => {
                        blocks.push(AnthropicContentBlock::Text {
                            text: text.clone(),
                            citations: None,
                        });
                    }
                    ResponsesInputPart::InputImage { image_url, .. } => {
                        // Try to extract base64 data URL or just skip for now.
                        if let Some(url_val) = image_url {
                            if let Some(url_str) = url_val.as_str() {
                                if url_str.starts_with("data:") {
                                    // Parse data URL: data:image/png;base64,XXXX
                                    if let Some(comma_pos) = url_str.find(',') {
                                        let meta = &url_str[5..comma_pos]; // after "data:"
                                        let data = &url_str[comma_pos + 1..];
                                        let format = meta
                                            .split(';')
                                            .next()
                                            .unwrap_or("image/png")
                                            .to_string();
                                        blocks.push(AnthropicContentBlock::Image {
                                            source: crate::translator::AnthropicImageSource {
                                                media_type: "base64".to_string(),
                                                format,
                                                data: data.to_string(),
                                            },
                                        });
                                    }
                                }
                            } else if let Some(obj) = url_val.as_object() {
                                // {"url": "data:...", "detail": "..."}
                                if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
                                    if url.starts_with("data:") {
                                        if let Some(comma_pos) = url.find(',') {
                                            let meta = &url[5..comma_pos];
                                            let data = &url[comma_pos + 1..];
                                            let format = meta
                                                .split(';')
                                                .next()
                                                .unwrap_or("image/png")
                                                .to_string();
                                            blocks.push(AnthropicContentBlock::Image {
                                                source: crate::translator::AnthropicImageSource {
                                                    media_type: "base64".to_string(),
                                                    format,
                                                    data: data.to_string(),
                                                },
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ResponsesInputPart::InputFile { .. } => {
                        // File inputs are not directly supported in Anthropic format; skip.
                    }
                    ResponsesInputPart::Unknown => {}
                }
            }
            if blocks.len() == 1 {
                if let AnthropicContentBlock::Text { text, citations: _ } = &blocks[0] {
                    return AnthropicContent::Text(text.clone());
                }
            }
            AnthropicContent::Blocks(blocks)
        }
    }
}

/// Translate Responses API tool_choice to Anthropic tool_choice.
fn translate_tool_choice(tc: &Value) -> Value {
    if let Some(s) = tc.as_str() {
        match s {
            "auto" => json!({"type": "auto"}),
            "none" => json!({"type": "none"}),
            "required" => json!({"type": "any"}),
            _ => json!({"type": "auto"}),
        }
    } else if tc.get("type").and_then(|v| v.as_str()) == Some("function") {
        let name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        json!({"type": "tool", "name": name})
    } else {
        json!({"type": "auto"})
    }
}

// ─── Translation: Anthropic internal → Responses API ─────────────────────────

/// Convert an Anthropic response to a Responses API response.
pub fn from_anthropic(
    anthropic_resp: &AnthropicResponse,
    req: &ResponsesRequest,
) -> ResponsesResponse {
    let resp_id = format!("resp_{}", Uuid::new_v4().to_string().replace('-', ""));
    let msg_id = format!("msg_{}", Uuid::new_v4().to_string().replace('-', ""));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut output_items = Vec::new();
    let mut function_calls = Vec::new();

    for block in &anthropic_resp.content {
        match block {
            AnthropicResponseContent::Text { text } => {
                output_items.push(ResponsesOutputItem::Message {
                    id: msg_id.clone(),
                    role: "assistant".to_string(),
                    content: vec![ResponsesOutputContent::OutputText {
                        text: text.clone(),
                        annotations: vec![],
                    }],
                    status: Some("completed".to_string()),
                });
            }
            AnthropicResponseContent::ToolUse { id, name, input } => {
                let args_str = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                function_calls.push(ResponsesOutputItem::FunctionCall {
                    id: format!("fc_{}", Uuid::new_v4().to_string().replace('-', "")),
                    call_id: id.clone(),
                    name: name.clone(),
                    arguments: args_str,
                    status: Some("completed".to_string()),
                });
            }
        }
    }

    // If no text output was produced, add an empty message.
    if output_items.is_empty() && function_calls.is_empty() {
        output_items.push(ResponsesOutputItem::Message {
            id: msg_id,
            role: "assistant".to_string(),
            content: vec![ResponsesOutputContent::OutputText {
                text: String::new(),
                annotations: vec![],
            }],
            status: Some("completed".to_string()),
        });
    }

    // Append function calls after messages.
    output_items.extend(function_calls);

    let status = match anthropic_resp.stop_reason.as_str() {
        "stop" | "tool_use" => "completed",
        "max_tokens" => "incomplete",
        _ => "completed",
    };

    let incomplete_details = if status == "incomplete" {
        Some(json!({"reason": "max_output_tokens"}))
    } else {
        None
    };

    ResponsesResponse {
        id: resp_id,
        object: "response".to_string(),
        created_at: now,
        model: anthropic_resp.model.clone(),
        output: output_items,
        usage: ResponsesUsage {
            input_tokens: anthropic_resp.usage.input_tokens,
            output_tokens: anthropic_resp.usage.output_tokens,
            total_tokens: anthropic_resp.usage.input_tokens + anthropic_resp.usage.output_tokens,
            input_tokens_details: None,
            output_tokens_details: None,
        },
        status: Some(status.to_string()),
        error: None,
        incomplete_details,
        metadata: req.metadata.clone(),
        temperature: req.temperature,
        top_p: req.top_p,
        max_output_tokens: req.max_output_tokens,
        previous_response_id: req.previous_response_id.clone(),
        reasoning: req.reasoning.clone(),
        text: req.text.clone(),
        tool_choice: req.tool_choice.clone(),
        tools: if req.tools.is_empty() {
            None
        } else {
            Some(
                req.tools
                    .iter()
                    .map(|t| serde_json::to_value(t).unwrap_or(json!({})))
                    .collect(),
            )
        },
        parallel_tool_calls: req.parallel_tool_calls,
        user: req.user.clone(),
    }
}

// ─── Streaming: Anthropic SSE → Responses API SSE ────────────────────────────

/// Generate the initial `response.created` SSE event.
pub fn stream_event_created(resp: &ResponsesResponse) -> String {
    format!(
        "event: response.created\ndata: {}\n\n",
        serde_json::to_string(resp).unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate the `response.in_progress` SSE event.
pub fn stream_event_in_progress(resp: &ResponsesResponse) -> String {
    let mut in_progress = resp.clone();
    in_progress.status = Some("in_progress".to_string());
    format!(
        "event: response.in_progress\ndata: {}\n\n",
        serde_json::to_string(&in_progress).unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.output_item.added` for a message item.
pub fn stream_event_output_item_added(msg_id: &str, role: &str) -> String {
    let item = json!({
        "type": "message",
        "id": msg_id,
        "role": role,
        "content": [],
        "status": "in_progress"
    });
    format!(
        "event: response.output_item.added\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "item": item
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.content_part.added` for an output_text part.
pub fn stream_event_content_part_added(_msg_id: &str) -> String {
    let part = json!({
        "type": "output_text",
        "text": "",
        "annotations": []
    });
    format!(
        "event: response.content_part.added\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "content_index": 0,
            "part": part
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.output_text.delta` for a text chunk.
pub fn stream_event_text_delta(delta: &str) -> String {
    format!(
        "event: response.output_text.delta\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "content_index": 0,
            "delta": delta
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.output_text.done` when text is complete.
pub fn stream_event_text_done(_msg_id: &str, full_text: &str) -> String {
    let part = json!({
        "type": "output_text",
        "text": full_text,
        "annotations": []
    });
    format!(
        "event: response.output_text.done\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "content_index": 0,
            "part": part
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.content_part.done`.
pub fn stream_event_content_part_done(_msg_id: &str, full_text: &str) -> String {
    let part = json!({
        "type": "output_text",
        "text": full_text,
        "annotations": []
    });
    format!(
        "event: response.content_part.done\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "content_index": 0,
            "part": part
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.output_item.done` for a completed message.
pub fn stream_event_output_item_done(msg_id: &str, full_text: &str) -> String {
    let item = json!({
        "type": "message",
        "id": msg_id,
        "role": "assistant",
        "content": [{"type": "output_text", "text": full_text, "annotations": []}],
        "status": "completed"
    });
    format!(
        "event: response.output_item.done\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "item": item
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.function_call_arguments.delta` for tool call args streaming.
pub fn stream_event_function_call_args_delta(call_id: &str, _name: &str, delta: &str) -> String {
    format!(
        "event: response.function_call_arguments.delta\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "item_id": format!("fc_{}", call_id),
            "delta": delta
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate `response.function_call_arguments.done` for a completed tool call.
pub fn stream_event_function_call_args_done(call_id: &str, name: &str, arguments: &str) -> String {
    let item = json!({
        "type": "function_call",
        "id": format!("fc_{}", call_id),
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
        "status": "completed"
    });
    format!(
        "event: response.function_call_arguments.done\ndata: {}\n\n",
        serde_json::to_string(&json!({
            "output_index": 0,
            "item_id": format!("fc_{}", call_id),
            "item": item
        }))
        .unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate the final `response.completed` SSE event.
pub fn stream_event_completed(resp: &ResponsesResponse) -> String {
    let mut completed = resp.clone();
    completed.status = Some("completed".to_string());
    format!(
        "event: response.completed\ndata: {}\n\n",
        serde_json::to_string(&completed).unwrap_or_else(|_| "{}".to_string())
    )
}

/// Generate a `response.failed` SSE event.
pub fn stream_event_failed(error_message: &str) -> String {
    let resp = json!({
        "id": format!("resp_{}", Uuid::new_v4().to_string().replace('-', "")),
        "object": "response",
        "created_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "model": "",
        "output": [],
        "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
        "status": "failed",
        "error": {
            "type": "server_error",
            "message": error_message
        }
    });
    format!(
        "event: response.failed\ndata: {}\n\n",
        serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string())
    )
}

// ─── SSE event parsing from upstream providers ───────────────────────────────

/// Extract text delta from an upstream SSE event (Anthropic or OpenAI format).
/// Returns `Some(text)` if this is a text delta event, `None` otherwise.
pub fn extract_text_delta_from_upstream(event: &str) -> Option<String> {
    // Find the data: line(s) in the event.
    for line in event.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                return None;
            }
            if let Ok(val) = serde_json::from_str::<Value>(data) {
                // Anthropic format: content_block_delta with text_delta
                if val.get("type").and_then(|v| v.as_str()) == Some("content_block_delta") {
                    if let Some(delta) = val.get("delta") {
                        if delta.get("type").and_then(|v| v.as_str()) == Some("text_delta") {
                            if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                return Some(text.to_string());
                            }
                        }
                    }
                }
                // OpenAI format: choices[0].delta.content
                if let Some(choices) = val.get("choices").and_then(|v| v.as_array()) {
                    if let Some(choice) = choices.first() {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                if !content.is_empty() {
                                    return Some(content.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract tool call information from an upstream SSE event.
/// Returns `Some((call_id, name, arguments_delta))` if this is a tool call delta.
pub fn extract_tool_call_delta_from_upstream(event: &str) -> Option<(String, String, String)> {
    for line in event.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                return None;
            }
            if let Ok(val) = serde_json::from_str::<Value>(data) {
                // OpenAI format: choices[0].delta.tool_calls
                if let Some(choices) = val.get("choices").and_then(|v| v.as_array()) {
                    if let Some(choice) = choices.first() {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(tool_calls) =
                                delta.get("tool_calls").and_then(|v| v.as_array())
                            {
                                if let Some(tc) = tool_calls.first() {
                                    let id = tc
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let name = tc
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let args = tc
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    if !args.is_empty() || !name.is_empty() {
                                        return Some((id, name, args));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if an upstream SSE event signals the end of the stream.
pub fn is_stream_done(event: &str) -> bool {
    event.contains("data: [DONE]")
        || event.contains("\"type\":\"message_stop\"")
        || event.contains("\"type\":\"message_delta\"")
            && event.contains("\"stop_reason\"")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string_input() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": "Hello, world!"
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        assert_eq!(req.model, "gpt-4o");
        match &req.input {
            ResponsesInput::Text(s) => assert_eq!(s, "Hello, world!"),
            _ => panic!("Expected text input"),
        }
    }

    #[test]
    fn test_parse_array_input() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": [
                {"type": "message", "role": "user", "content": "Hello"},
                {"type": "message", "role": "assistant", "content": "Hi there!"},
                {"type": "message", "role": "user", "content": "How are you?"}
            ]
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        match &req.input {
            ResponsesInput::Items(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected items input"),
        }
    }

    #[test]
    fn test_to_anthropic_string_input() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": "Hello, world!",
            "instructions": "You are a helpful assistant."
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        assert_eq!(anthropic.model, "gpt-4o");
        assert_eq!(anthropic.system, Some("You are a helpful assistant.".to_string()));
        assert_eq!(anthropic.messages.len(), 1);
        assert_eq!(anthropic.messages[0].role, "user");
    }

    #[test]
    fn test_to_anthropic_with_tools() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": "What's the weather?",
            "tools": [
                {
                    "type": "function",
                    "name": "get_weather",
                    "description": "Get the weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        }
                    }
                }
            ]
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        assert_eq!(anthropic.tools.len(), 1);
        assert_eq!(anthropic.tools[0].name, "get_weather");
        assert_eq!(anthropic.tools[0].description, Some("Get the weather".to_string()));
    }

    #[test]
    fn test_to_anthropic_with_function_call_output() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": [
                {"type": "message", "role": "user", "content": "What's the weather?"},
                {"type": "function_call", "call_id": "call_123", "name": "get_weather", "arguments": "{\"location\":\"SF\"}"},
                {"type": "function_call_output", "call_id": "call_123", "output": "Sunny, 72°F"}
            ]
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        // user message + assistant tool_use + user tool_result
        assert_eq!(anthropic.messages.len(), 3);
        assert_eq!(anthropic.messages[1].role, "assistant");
        assert_eq!(anthropic.messages[2].role, "user");
    }

    #[test]
    fn test_to_anthropic_system_role() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": [
                {"type": "message", "role": "developer", "content": "Be concise."},
                {"type": "message", "role": "user", "content": "Hello"}
            ]
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        assert_eq!(anthropic.system, Some("Be concise.".to_string()));
        assert_eq!(anthropic.messages.len(), 1);
        assert_eq!(anthropic.messages[0].role, "user");
    }

    #[test]
    fn test_from_anthropic_text_response() {
        let anthropic_resp = AnthropicResponse {
            id: "msg_test".to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            model: "gpt-4o".to_string(),
            content: vec![AnthropicResponseContent::Text {
                text: "Hello! How can I help?".to_string(),
            }],
            stop_reason: "stop".to_string(),
            stop_sequence: None,
            usage: crate::translator::AnthropicUsage {
                input_tokens: 10,
                output_tokens: 8,
            },
        };
        let req_json = json!({
            "model": "gpt-4o",
            "input": "Hi"
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let resp = from_anthropic(&anthropic_resp, &req);

        assert_eq!(resp.object, "response");
        assert_eq!(resp.status, Some("completed".to_string()));
        assert_eq!(resp.output.len(), 1);
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 8);
        assert_eq!(resp.usage.total_tokens, 18);
    }

    #[test]
    fn test_from_anthropic_tool_use_response() {
        let anthropic_resp = AnthropicResponse {
            id: "msg_test".to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            model: "gpt-4o".to_string(),
            content: vec![
                AnthropicResponseContent::Text {
                    text: "Let me check the weather.".to_string(),
                },
                AnthropicResponseContent::ToolUse {
                    id: "call_123".to_string(),
                    name: "get_weather".to_string(),
                    input: json!({"location": "SF"}),
                },
            ],
            stop_reason: "tool_use".to_string(),
            stop_sequence: None,
            usage: crate::translator::AnthropicUsage {
                input_tokens: 20,
                output_tokens: 15,
            },
        };
        let req_json = json!({
            "model": "gpt-4o",
            "input": "Weather?"
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let resp = from_anthropic(&anthropic_resp, &req);

        assert_eq!(resp.status, Some("completed".to_string()));
        // Should have 1 message + 1 function call
        assert_eq!(resp.output.len(), 2);
    }

    #[test]
    fn test_extract_text_delta_anthropic() {
        let event = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n";
        let delta = extract_text_delta_from_upstream(event);
        assert_eq!(delta, Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_text_delta_openai() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"World\"}}]}\n\n";
        let delta = extract_text_delta_from_upstream(event);
        assert_eq!(delta, Some("World".to_string()));
    }

    #[test]
    fn test_extract_text_delta_empty() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n";
        let delta = extract_text_delta_from_upstream(event);
        assert_eq!(delta, None);
    }

    #[test]
    fn test_is_stream_done() {
        assert!(is_stream_done("data: [DONE]\n\n"));
        assert!(is_stream_done("data: {\"type\":\"message_stop\"}\n\n"));
        assert!(!is_stream_done("data: {\"type\":\"content_block_delta\"}\n\n"));
    }

    #[test]
    fn test_stream_events_format() {
        let resp = ResponsesResponse {
            id: "resp_test".to_string(),
            object: "response".to_string(),
            created_at: 1234567890,
            model: "gpt-4o".to_string(),
            output: vec![],
            usage: ResponsesUsage {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                input_tokens_details: None,
                output_tokens_details: None,
            },
            status: Some("in_progress".to_string()),
            error: None,
            incomplete_details: None,
            metadata: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            previous_response_id: None,
            reasoning: None,
            text: None,
            tool_choice: None,
            tools: None,
            parallel_tool_calls: None,
            user: None,
        };

        let created = stream_event_created(&resp);
        assert!(created.starts_with("event: response.created\ndata: "));
        assert!(created.ends_with("\n\n"));

        let delta = stream_event_text_delta("Hello");
        assert!(delta.contains("event: response.output_text.delta"));
        assert!(delta.contains("\"delta\":\"Hello\""));

        let done = stream_event_text_done("msg_123", "Hello World");
        assert!(done.contains("event: response.output_text.done"));
        assert!(done.contains("\"text\":\"Hello World\""));

        let completed = stream_event_completed(&resp);
        assert!(completed.contains("event: response.completed"));
    }

    #[test]
    fn test_tool_choice_translation() {
        // "auto" → {"type": "auto"}
        let tc = json!("auto");
        let result = translate_tool_choice(&tc);
        assert_eq!(result, json!({"type": "auto"}));

        // "required" → {"type": "any"}
        let tc = json!("required");
        let result = translate_tool_choice(&tc);
        assert_eq!(result, json!({"type": "any"}));

        // "none" → {"type": "none"}
        let tc = json!("none");
        let result = translate_tool_choice(&tc);
        assert_eq!(result, json!({"type": "none"}));

        // function-specific
        let tc = json!({"type": "function", "function": {"name": "get_weather"}});
        let result = translate_tool_choice(&tc);
        assert_eq!(result, json!({"type": "tool", "name": "get_weather"}));
    }

    #[test]
    fn test_input_content_with_parts() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Describe this image"},
                        {"type": "input_text", "text": "in detail"}
                    ]
                }
            ]
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        assert_eq!(anthropic.messages.len(), 1);
        // Content should be blocks since there are multiple parts.
        match &anthropic.messages[0].content {
            Some(AnthropicContent::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 2);
            }
            _ => panic!("Expected blocks content"),
        }
    }

    #[test]
    fn test_max_output_tokens_mapping() {
        let req_json = json!({
            "model": "gpt-4o",
            "input": "Hello",
            "max_output_tokens": 4096,
            "temperature": 0.7,
            "top_p": 0.95
        });
        let req: ResponsesRequest = serde_json::from_value(req_json).unwrap();
        let anthropic = to_anthropic(&req);

        assert_eq!(anthropic.max_tokens, Some(4096));
        assert_eq!(anthropic.temperature, Some(0.7));
        assert_eq!(anthropic.top_p, Some(0.95));
    }
}
