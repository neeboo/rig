use std::convert::Infallible;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use crate::{completion, message, OneOrMany};
use crate::completion::{CompletionError, MessageError};
#[derive(Debug, Deserialize)]
pub struct CompletionResponse {
    pub content: Vec<Content>,
    pub id: String,
    pub model: String,
    pub role: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub output_tokens: u64,
}

impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Input tokens: {}\nCache read input tokens: {}\nCache creation input tokens: {}\nOutput tokens: {}",
            self.input_tokens,
            match self.cache_read_input_tokens {
                Some(token) => token.to_string(),
                None => "n/a".to_string(),
            },
            match self.cache_creation_input_tokens {
                Some(token) => token.to_string(),
                None => "n/a".to_string(),
            },
            self.output_tokens
        )
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    #[default]
    Auto,
    Any,
    Tool {
        name: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheControl {
    Ephemeral,
}

impl TryFrom<CompletionResponse> for completion::CompletionResponse<CompletionResponse> {
    type Error = CompletionError;

    fn try_from(response: CompletionResponse) -> Result<Self, Self::Error> {
        let content = response
            .content
            .iter()
            .map(|content| {
                Ok(match content {
                    Content::Text { text } => completion::AssistantContent::text(text),
                    Content::ToolUse { id, name, input } => {
                        completion::AssistantContent::tool_call(id, name, input.clone())
                    }
                    _ => {
                        return Err(CompletionError::ResponseError(
                            "Response did not contain a message or tool call".into(),
                        ))
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let choice = OneOrMany::many(content).map_err(|_| {
            CompletionError::ResponseError(
                "Response contained no message or tool call (empty)".to_owned(),
            )
        })?;

        Ok(completion::CompletionResponse {
            choice,
            raw_response: response,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    #[serde(deserialize_with = "string_or_one_or_many")]
    pub content: OneOrMany<Content>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(deserialize_with = "string_or_one_or_many")]
        content: OneOrMany<ToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Document {
        source: DocumentSource,
    },
}

impl FromStr for Content {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Content::Text { text: s.to_owned() })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text { text: String },
    Image(ImageSource),
}

impl FromStr for ToolResultContent {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ToolResultContent::Text { text: s.to_owned() })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ImageSource {
    pub data: String,
    pub media_type: ImageFormat,
    pub r#type: SourceType,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct DocumentSource {
    pub data: String,
    pub media_type: DocumentFormat,
    pub r#type: SourceType,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[serde(rename = "image/jpeg")]
    JPEG,
    #[serde(rename = "image/png")]
    PNG,
    #[serde(rename = "image/gif")]
    GIF,
    #[serde(rename = "image/webp")]
    WEBP,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    #[serde(rename = "application/pdf")]
    PDF,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    BASE64,
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Content::Text { text }
    }
}

impl From<String> for ToolResultContent {
    fn from(text: String) -> Self {
        ToolResultContent::Text { text }
    }
}

impl TryFrom<message::ContentFormat> for SourceType {
    type Error = MessageError;

    fn try_from(format: message::ContentFormat) -> Result<Self, Self::Error> {
        match format {
            message::ContentFormat::Base64 => Ok(SourceType::BASE64),
            message::ContentFormat::String => Err(MessageError::ConversionError(
                "Image urls are not supported in Anthropic".to_owned(),
            )),
        }
    }
}

impl From<SourceType> for message::ContentFormat {
    fn from(source_type: SourceType) -> Self {
        match source_type {
            SourceType::BASE64 => message::ContentFormat::Base64,
        }
    }
}

impl TryFrom<message::ImageMediaType> for ImageFormat {
    type Error = MessageError;

    fn try_from(media_type: message::ImageMediaType) -> Result<Self, Self::Error> {
        Ok(match media_type {
            message::ImageMediaType::JPEG => ImageFormat::JPEG,
            message::ImageMediaType::PNG => ImageFormat::PNG,
            message::ImageMediaType::GIF => ImageFormat::GIF,
            message::ImageMediaType::WEBP => ImageFormat::WEBP,
            _ => {
                return Err(MessageError::ConversionError(
                    format!("Unsupported image media type: {:?}", media_type).to_owned(),
                ))
            }
        })
    }
}

impl From<ImageFormat> for message::ImageMediaType {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::JPEG => message::ImageMediaType::JPEG,
            ImageFormat::PNG => message::ImageMediaType::PNG,
            ImageFormat::GIF => message::ImageMediaType::GIF,
            ImageFormat::WEBP => message::ImageMediaType::WEBP,
        }
    }
}

impl From<message::AssistantContent> for Content {
    fn from(text: message::AssistantContent) -> Self {
        match text {
            message::AssistantContent::Text(message::Text { text }) => Content::Text { text },
            message::AssistantContent::ToolCall(message::ToolCall { id, function }) => {
                Content::ToolUse {
                    id,
                    name: function.name,
                    input: function.arguments,
                }
            }
        }
    }
}

impl TryFrom<message::Message> for Message {
    type Error = MessageError;

    fn try_from(message: message::Message) -> Result<Self, Self::Error> {
        Ok(match message {
            message::Message::User { content } => Message {
                role: Role::User,
                content: content.try_map(|content| match content {
                    message::UserContent::Text(message::Text { text }) => {
                        Ok(Content::Text { text })
                    }
                    message::UserContent::ToolResult(message::ToolResult { id, content }) => {
                        Ok(Content::ToolResult {
                            tool_use_id: id,
                            content: content.try_map(|content| match content {
                                message::ToolResultContent::Text(message::Text { text }) => {
                                    Ok(ToolResultContent::Text { text })
                                }
                                message::ToolResultContent::Image(image) => {
                                    let media_type =
                                        image.media_type.ok_or(MessageError::ConversionError(
                                            "Image media type is required".to_owned(),
                                        ))?;
                                    let format =
                                        image.format.ok_or(MessageError::ConversionError(
                                            "Image format is required".to_owned(),
                                        ))?;
                                    Ok(ToolResultContent::Image(ImageSource {
                                        data: image.data,
                                        media_type: media_type.try_into()?,
                                        r#type: format.try_into()?,
                                    }))
                                }
                            })?,
                            is_error: None,
                        })
                    }
                    message::UserContent::Image(message::Image {
                                                    data,
                                                    format,
                                                    media_type,
                                                    ..
                                                }) => {
                        let source = ImageSource {
                            data,
                            media_type: match media_type {
                                Some(media_type) => media_type.try_into()?,
                                None => {
                                    return Err(MessageError::ConversionError(
                                        "Image media type is required".to_owned(),
                                    ))
                                }
                            },
                            r#type: match format {
                                Some(format) => format.try_into()?,
                                None => SourceType::BASE64,
                            },
                        };
                        Ok(Content::Image { source })
                    }
                    message::UserContent::Document(message::Document { data, format, .. }) => {
                        let source = DocumentSource {
                            data,
                            media_type: DocumentFormat::PDF,
                            r#type: match format {
                                Some(format) => format.try_into()?,
                                None => SourceType::BASE64,
                            },
                        };
                        Ok(Content::Document { source })
                    }
                    message::UserContent::Audio { .. } => Err(MessageError::ConversionError(
                        "Audio is not supported in Anthropic".to_owned(),
                    )),
                })?,
            },

            message::Message::Assistant { content } => Message {
                content: content.map(|content| content.into()),
                role: Role::Assistant,
            },
        })
    }
}

impl TryFrom<Content> for message::AssistantContent {
    type Error = MessageError;

    fn try_from(content: Content) -> Result<Self, Self::Error> {
        Ok(match content {
            Content::Text { text } => message::AssistantContent::text(text),
            Content::ToolUse { id, name, input } => {
                message::AssistantContent::tool_call(id, name, input)
            }
            _ => {
                return Err(MessageError::ConversionError(
                    format!("Unsupported content type for Assistant role: {:?}", content)
                        .to_owned(),
                ))
            }
        })
    }
}

impl From<ToolResultContent> for message::ToolResultContent {
    fn from(content: ToolResultContent) -> Self {
        match content {
            ToolResultContent::Text { text } => message::ToolResultContent::text(text),
            ToolResultContent::Image(ImageSource {
                                         data,
                                         media_type: format,
                                         r#type,
                                     }) => message::ToolResultContent::image(
                data,
                Some(r#type.into()),
                Some(format.into()),
                None,
            ),
        }
    }
}

impl TryFrom<Message> for message::Message {
    type Error = MessageError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        Ok(match message.role {
            Role::User => message::Message::User {
                content: message.content.try_map(|content| {
                    Ok(match content {
                        Content::Text { text } => message::UserContent::text(text),
                        Content::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => message::UserContent::tool_result(
                            tool_use_id,
                            content.map(|content| content.into()),
                        ),
                        Content::Image { source } => message::UserContent::Image(message::Image {
                            data: source.data,
                            format: Some(message::ContentFormat::Base64),
                            media_type: Some(source.media_type.into()),
                            detail: None,
                        }),
                        Content::Document { source } => message::UserContent::document(
                            source.data,
                            Some(message::ContentFormat::Base64),
                            Some(message::DocumentMediaType::PDF),
                        ),
                        _ => {
                            return Err(MessageError::ConversionError(
                                "Unsupported content type for User role".to_owned(),
                            ))
                        }
                    })
                })?,
            },
            Role::Assistant => match message.content.first() {
                Content::Text { .. } | Content::ToolUse { .. } => message::Message::Assistant {
                    content: message.content.try_map(|content| content.try_into())?,
                },

                _ => {
                    return Err(MessageError::ConversionError(
                        format!("Unsupported message for Assistant role: {:?}", message).to_owned(),
                    ))
                }
            },
        })
    }
}