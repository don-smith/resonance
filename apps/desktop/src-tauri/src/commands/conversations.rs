use std::collections::BTreeMap;

use resonance_runtime::{
    conversations::{
        ChannelView, ConversationError, ConversationRuntime, ConversationRuntimeError,
        ConversationSyncState, MessageView, MAX_CHANNEL_NAME_BYTES, MAX_CHANNEL_SNAPSHOT,
        MAX_MARKDOWN_BYTES,
    },
    identity::PublicIdentity,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use super::workspace::ManagedWorkspaceState;

const MAX_IDENTIFIER_LENGTH: usize = 128;
const MAX_PAGE_SIZE: usize = 100;
const MAX_DISPLAY_NAME_LENGTH: usize = 255;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ConversationsRequest {
    Snapshot,
    Messages {
        channel_id: String,
        cursor: Option<String>,
        limit: usize,
    },
    CreateChannel {
        name: String,
    },
    RenameChannel {
        channel_id: String,
        name: String,
    },
    ArchiveChannel {
        channel_id: String,
    },
    PostMessage {
        channel_id: String,
        markdown: String,
    },
    MarkRead {
        channel_id: String,
        message_id: String,
    },
    SynchronizationState,
}

impl ConversationsRequest {
    fn parse(value: Value) -> Result<Self, ConversationsError> {
        let object = value.as_object().ok_or_else(invalid_request)?;
        let operation = object
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(invalid_request)?;
        let allowed: &[&str] = match operation {
            "snapshot" | "synchronization-state" => &["operation"],
            "messages" => &["operation", "channelId", "cursor", "limit"],
            "create-channel" => &["operation", "name"],
            "rename-channel" => &["operation", "channelId", "name"],
            "archive-channel" => &["operation", "channelId"],
            "post-message" => &["operation", "channelId", "markdown"],
            "mark-read" => &["operation", "channelId", "messageId"],
            _ => return Err(invalid_request()),
        };
        if object.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(invalid_request());
        }
        let request: Self = serde_json::from_value(value).map_err(|_| invalid_request())?;
        request
            .validate()
            .then_some(request)
            .ok_or_else(invalid_request)
    }

    fn validate(&self) -> bool {
        match self {
            Self::Snapshot | Self::SynchronizationState => true,
            Self::Messages {
                channel_id,
                cursor,
                limit,
            } => {
                decode_identifier::<16>(channel_id).is_some()
                    && cursor
                        .as_ref()
                        .is_none_or(|value| decode_identifier::<32>(value).is_some())
                    && (1..=MAX_PAGE_SIZE).contains(limit)
            }
            Self::CreateChannel { name } => valid_channel_name(name),
            Self::RenameChannel { channel_id, name } => {
                decode_identifier::<16>(channel_id).is_some() && valid_channel_name(name)
            }
            Self::ArchiveChannel { channel_id } => decode_identifier::<16>(channel_id).is_some(),
            Self::PostMessage {
                channel_id,
                markdown,
            } => {
                decode_identifier::<16>(channel_id).is_some()
                    && markdown.len() <= MAX_MARKDOWN_BYTES
            }
            Self::MarkRead {
                channel_id,
                message_id,
            } => {
                decode_identifier::<16>(channel_id).is_some()
                    && decode_identifier::<32>(message_id).is_some()
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ConversationsResponse {
    Snapshot {
        snapshot: ConversationsSnapshot,
    },
    Messages {
        page: ConversationMessagePage,
    },
    CreateChannel {
        channel: ConversationChannelView,
    },
    RenameChannel {
        channel: ConversationChannelView,
    },
    ArchiveChannel {
        channel: ConversationChannelView,
    },
    PostMessage {
        message: ConversationMessageView,
    },
    MarkRead {
        channel_id: String,
        unread_count: usize,
    },
    SynchronizationState {
        synchronization: SynchronizationView,
    },
}

impl ConversationsResponse {
    fn validate(&self) -> bool {
        match self {
            Self::Snapshot { snapshot } => snapshot.validate(),
            Self::Messages { page } => page.validate(),
            Self::CreateChannel { channel }
            | Self::RenameChannel { channel }
            | Self::ArchiveChannel { channel } => channel.validate(),
            Self::PostMessage { message } => message.validate(),
            Self::MarkRead { channel_id, .. } => valid_identifier(channel_id),
            Self::SynchronizationState { .. } => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationsSnapshot {
    pub workspace_id: String,
    pub channels: Vec<ConversationChannelView>,
    pub synchronization: SynchronizationView,
}

impl ConversationsSnapshot {
    fn validate(&self) -> bool {
        valid_identifier(&self.workspace_id)
            && self.channels.len() <= MAX_CHANNEL_SNAPSHOT
            && self.channels.iter().all(ConversationChannelView::validate)
            && self
                .channels
                .iter()
                .map(|channel| channel.channel_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.channels.len()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationChannelView {
    pub channel_id: String,
    pub name: String,
    pub archived: bool,
    pub unread_count: usize,
    pub can_manage: bool,
}

impl ConversationChannelView {
    fn validate(&self) -> bool {
        valid_identifier(&self.channel_id) && valid_channel_name(&self.name)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessagePage {
    pub channel_id: String,
    pub messages: Vec<ConversationMessageView>,
    pub next_cursor: Option<String>,
}

impl ConversationMessagePage {
    fn validate(&self) -> bool {
        valid_identifier(&self.channel_id)
            && self.messages.len() <= MAX_PAGE_SIZE
            && self
                .messages
                .iter()
                .all(|message| message.validate() && message.channel_id == self.channel_id)
            && self
                .messages
                .iter()
                .map(|message| message.message_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.messages.len()
            && self
                .next_cursor
                .as_ref()
                .is_none_or(|value| valid_identifier(value))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessageView {
    pub message_id: String,
    pub channel_id: String,
    pub author: ConversationAuthorView,
    pub created_at: i64,
    pub markdown: String,
}

impl ConversationMessageView {
    fn validate(&self) -> bool {
        valid_identifier(&self.message_id)
            && valid_identifier(&self.channel_id)
            && self.author.validate()
            && self.created_at.unsigned_abs() <= 9_007_199_254_740_991
            && self.markdown.len() <= MAX_MARKDOWN_BYTES
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationAuthorView {
    pub public_identity: String,
    pub display_name: String,
}

impl ConversationAuthorView {
    fn validate(&self) -> bool {
        valid_identifier(&self.public_identity)
            && valid_text(&self.display_name, MAX_DISPLAY_NAME_LENGTH)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SynchronizationView {
    Offline,
    WaitingToSync,
    Current,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationsError {
    pub code: ConversationsErrorCode,
    pub message: String,
}

impl ConversationsError {
    fn new(code: ConversationsErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
        }
    }
    #[cfg(test)]
    fn validate(&self) -> bool {
        self.message == self.code.message()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConversationsErrorCode {
    UnavailableCapability,
    InvalidRequest,
    MissingChannel,
    ArchivedChannel,
    Unauthorized,
    MissingEpoch,
    AuthoringBlocked,
    SizeLimit,
    Internal,
}

impl ConversationsErrorCode {
    fn message(self) -> &'static str {
        match self {
            Self::UnavailableCapability => "Conversations are unavailable.",
            Self::InvalidRequest => "The conversations request is invalid.",
            Self::MissingChannel => "That channel is unavailable.",
            Self::ArchivedChannel => "That channel is archived.",
            Self::Unauthorized => "You cannot perform that conversation action.",
            Self::MissingEpoch => "Conversation encryption is not ready yet.",
            Self::AuthoringBlocked => "Conversation authoring is waiting for membership to update.",
            Self::SizeLimit => "The conversations size limit was exceeded.",
            Self::Internal => "Conversations could not complete the request.",
        }
    }
}

#[tauri::command]
pub async fn conversations_v1(
    request: Value,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<ConversationsResponse, ConversationsError> {
    let request = ConversationsRequest::parse(request)?;
    let response = dispatch(request, &state).await?;
    if !response.validate() {
        return Err(ConversationsError::new(ConversationsErrorCode::Internal));
    }
    if let Some((channel_id, message_id)) = invalidation_for_response(&response) {
        let workspace_id = state
            .inner
            .with_application(|application| {
                application
                    .view()
                    .workspace
                    .map(|workspace| workspace.id.as_str().to_owned())
            })
            .await
            .ok_or_else(unavailable)?;
        state
            .inner
            .emit_conversation_changed(workspace_id, channel_id, message_id);
    }
    Ok(response)
}

async fn dispatch(
    request: ConversationsRequest,
    state: &ManagedWorkspaceState,
) -> Result<ConversationsResponse, ConversationsError> {
    state
        .inner
        .with_application(|application| {
            let view = application.view();
            let workspace_id = view
                .workspace
                .as_ref()
                .map(|workspace| workspace.id.as_str().to_owned())
                .ok_or_else(unavailable)?;
            let local_identity = view.local_public_identity.ok_or_else(unavailable)?;
            let names = view
                .members
                .into_iter()
                .map(|member| (member.public_identity, member.display_name))
                .collect::<BTreeMap<_, _>>();
            match request {
                ConversationsRequest::Snapshot => {
                    let synchronization = application
                        .conversation_synchronization_state()
                        .map(sync_view)
                        .map_err(runtime_error)?;
                    let conversation = application.conversation().ok_or_else(unavailable)?;
                    let channels = conversation
                        .channels()
                        .into_iter()
                        .map(|channel| channel_view(conversation, channel, local_identity))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(ConversationsResponse::Snapshot {
                        snapshot: ConversationsSnapshot {
                            workspace_id,
                            channels,
                            synchronization,
                        },
                    })
                }
                ConversationsRequest::Messages {
                    channel_id,
                    cursor,
                    limit,
                } => {
                    let channel_id =
                        decode_identifier::<16>(&channel_id).ok_or_else(invalid_request)?;
                    let conversation = application.conversation().ok_or_else(unavailable)?;
                    if !conversation
                        .channels()
                        .iter()
                        .any(|channel| channel.channel_id == channel_id)
                    {
                        return Err(ConversationsError::new(
                            ConversationsErrorCode::MissingChannel,
                        ));
                    }
                    let all = conversation.messages(&channel_id).map_err(runtime_error)?;
                    let start = match cursor {
                        None => 0,
                        Some(cursor) => {
                            let cursor =
                                decode_identifier::<32>(&cursor).ok_or_else(invalid_request)?;
                            all.iter()
                                .position(|message| message.message_id == cursor)
                                .map(|index| index + 1)
                                .ok_or_else(invalid_request)?
                        }
                    };
                    let more = start.saturating_add(limit) < all.len();
                    let messages = all
                        .into_iter()
                        .skip(start)
                        .take(limit)
                        .map(|message| message_view(message, &names))
                        .collect::<Vec<_>>();
                    let next_cursor = more
                        .then(|| messages.last().map(|message| message.message_id.clone()))
                        .flatten();
                    Ok(ConversationsResponse::Messages {
                        page: ConversationMessagePage {
                            channel_id: encode_identifier(&channel_id),
                            messages,
                            next_cursor,
                        },
                    })
                }
                ConversationsRequest::CreateChannel { name } => {
                    let channel = application
                        .conversation_mut()
                        .ok_or_else(unavailable)?
                        .create_channel(name, unix_seconds())
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::CreateChannel {
                        channel: channel_view(
                            application.conversation().ok_or_else(unavailable)?,
                            channel,
                            local_identity,
                        )?,
                    })
                }
                ConversationsRequest::RenameChannel { channel_id, name } => {
                    let channel_id = decode_identifier(&channel_id).ok_or_else(invalid_request)?;
                    require_open_channel(
                        application.conversation().ok_or_else(unavailable)?,
                        channel_id,
                    )?;
                    let channel = application
                        .conversation_mut()
                        .ok_or_else(unavailable)?
                        .rename_channel(channel_id, name, unix_seconds())
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::RenameChannel {
                        channel: channel_view(
                            application.conversation().ok_or_else(unavailable)?,
                            channel,
                            local_identity,
                        )?,
                    })
                }
                ConversationsRequest::ArchiveChannel { channel_id } => {
                    let channel_id = decode_identifier(&channel_id).ok_or_else(invalid_request)?;
                    require_open_channel(
                        application.conversation().ok_or_else(unavailable)?,
                        channel_id,
                    )?;
                    let channel = application
                        .conversation_mut()
                        .ok_or_else(unavailable)?
                        .archive_channel(channel_id, unix_seconds())
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::ArchiveChannel {
                        channel: channel_view(
                            application.conversation().ok_or_else(unavailable)?,
                            channel,
                            local_identity,
                        )?,
                    })
                }
                ConversationsRequest::PostMessage {
                    channel_id,
                    markdown,
                } => {
                    let channel_id = decode_identifier(&channel_id).ok_or_else(invalid_request)?;
                    require_open_channel(
                        application.conversation().ok_or_else(unavailable)?,
                        channel_id,
                    )?;
                    let message = application
                        .conversation_mut()
                        .ok_or_else(unavailable)?
                        .post_message(channel_id, &markdown, unix_seconds())
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::PostMessage {
                        message: message_view(message, &names),
                    })
                }
                ConversationsRequest::MarkRead {
                    channel_id,
                    message_id,
                } => {
                    let channel = decode_identifier(&channel_id).ok_or_else(invalid_request)?;
                    application
                        .conversation()
                        .ok_or_else(unavailable)?
                        .mark_read(
                            channel,
                            decode_identifier(&message_id).ok_or_else(invalid_request)?,
                        )
                        .map_err(runtime_error)?;
                    let unread_count = application
                        .conversation()
                        .ok_or_else(unavailable)?
                        .unread_count(channel)
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::MarkRead {
                        channel_id: encode_identifier(&channel),
                        unread_count,
                    })
                }
                ConversationsRequest::SynchronizationState => {
                    let synchronization = application
                        .conversation_synchronization_state()
                        .map(sync_view)
                        .map_err(runtime_error)?;
                    Ok(ConversationsResponse::SynchronizationState { synchronization })
                }
            }
        })
        .await
}

fn require_open_channel(
    runtime: &ConversationRuntime,
    channel_id: [u8; 16],
) -> Result<(), ConversationsError> {
    let Some(channel) = runtime
        .channels()
        .into_iter()
        .find(|channel| channel.channel_id == channel_id)
    else {
        return Err(ConversationsError::new(
            ConversationsErrorCode::MissingChannel,
        ));
    };
    if channel.archived {
        return Err(ConversationsError::new(
            ConversationsErrorCode::ArchivedChannel,
        ));
    }
    Ok(())
}

fn channel_view(
    runtime: &ConversationRuntime,
    channel: ChannelView,
    local: PublicIdentity,
) -> Result<ConversationChannelView, ConversationsError> {
    Ok(ConversationChannelView {
        channel_id: encode_identifier(&channel.channel_id),
        name: channel.name,
        archived: channel.archived,
        unread_count: runtime
            .unread_count(channel.channel_id)
            .map_err(runtime_error)?,
        can_manage: channel.creator == local,
    })
}

fn message_view(
    message: MessageView,
    names: &BTreeMap<PublicIdentity, String>,
) -> ConversationMessageView {
    let identity = message.author.to_string();
    ConversationMessageView {
        message_id: encode_identifier(&message.message_id),
        channel_id: encode_identifier(&message.channel_id),
        author: ConversationAuthorView {
            public_identity: identity.clone(),
            display_name: names
                .get(&message.author)
                .cloned()
                .unwrap_or_else(|| format!("Member {}", &identity[..identity.len().min(12)])),
        },
        created_at: message.created_at,
        markdown: message.markdown,
    }
}

fn invalidation_for_response(response: &ConversationsResponse) -> Option<(String, Option<String>)> {
    match response {
        ConversationsResponse::CreateChannel { channel }
        | ConversationsResponse::RenameChannel { channel }
        | ConversationsResponse::ArchiveChannel { channel } => {
            Some((channel.channel_id.clone(), None))
        }
        ConversationsResponse::PostMessage { message } => {
            Some((message.channel_id.clone(), Some(message.message_id.clone())))
        }
        ConversationsResponse::MarkRead { channel_id, .. } => Some((channel_id.clone(), None)),
        _ => None,
    }
}

fn runtime_error(error: ConversationRuntimeError) -> ConversationsError {
    let code = match error {
        ConversationRuntimeError::MissingCurrentEpoch => ConversationsErrorCode::MissingEpoch,
        ConversationRuntimeError::LocalAuthoringBlocked => ConversationsErrorCode::AuthoringBlocked,
        ConversationRuntimeError::Conversation(ConversationError::SizeLimit { .. }) => {
            ConversationsErrorCode::SizeLimit
        }
        ConversationRuntimeError::Conversation(ConversationError::UnauthorizedData(
            "channel is unavailable or archived",
        )) => ConversationsErrorCode::ArchivedChannel,
        ConversationRuntimeError::Conversation(ConversationError::UnauthorizedData(_)) => {
            ConversationsErrorCode::Unauthorized
        }
        _ => ConversationsErrorCode::Internal,
    };
    ConversationsError::new(code)
}

fn invalid_request() -> ConversationsError {
    ConversationsError::new(ConversationsErrorCode::InvalidRequest)
}
fn unavailable() -> ConversationsError {
    ConversationsError::new(ConversationsErrorCode::UnavailableCapability)
}
fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= MAX_IDENTIFIER_LENGTH
}
fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

fn valid_channel_name(value: &str) -> bool {
    value.starts_with('#') && value.len() > 1 && valid_text(value, MAX_CHANNEL_NAME_BYTES)
}
fn sync_view(state: ConversationSyncState) -> SynchronizationView {
    match state {
        ConversationSyncState::Offline => SynchronizationView::Offline,
        ConversationSyncState::WaitingToSync => SynchronizationView::WaitingToSync,
        ConversationSyncState::Current => SynchronizationView::Current,
    }
}
fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().try_into().unwrap_or(i64::MAX))
        .unwrap_or(0)
}
fn encode_identifier(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode_identifier<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.is_ascii() {
        return None;
    }
    let mut output = [0; N];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        output[index] = ((high << 4) | low) as u8;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CORPUS: &str = include_str!(
        "../../../../../packages/contracts/fixtures/conversations-v1/valid/corpus.json"
    );
    const INVALID_CORPUS: &str = include_str!(
        "../../../../../packages/contracts/fixtures/conversations-v1/invalid/corpus.json"
    );

    #[test]
    fn request_contract_accepts_only_valid_request_fixtures() {
        let values: Vec<Value> = serde_json::from_str(VALID_CORPUS).unwrap();
        for envelope in values
            .into_iter()
            .filter(|value| value["kind"] == "request")
        {
            assert!(ConversationsRequest::parse(envelope["value"].clone()).is_ok());
        }
        let invalid: Vec<Value> = serde_json::from_str(INVALID_CORPUS).unwrap();
        for envelope in invalid
            .into_iter()
            .filter(|value| value["value"]["kind"] == "request")
        {
            assert!(ConversationsRequest::parse(envelope["value"]["value"].clone()).is_err());
        }
    }

    #[test]
    fn response_and_error_fixtures_share_the_typescript_contract() {
        let values: Vec<Value> = serde_json::from_str(VALID_CORPUS).unwrap();
        for envelope in values {
            match envelope["kind"].as_str() {
                Some("response") => {
                    let response: ConversationsResponse =
                        serde_json::from_value(envelope["value"].clone()).unwrap();
                    assert!(response.validate());
                }
                Some("error") => {
                    let error: ConversationsError =
                        serde_json::from_value(envelope["value"].clone()).unwrap();
                    assert!(error.validate());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn rejects_malformed_and_oversized_semantic_input() {
        assert!(ConversationsRequest::parse(serde_json::json!(null)).is_err());
        assert!(ConversationsRequest::parse(serde_json::json!({
            "operation": "messages",
            "channelId": "000102030405060708090a0b0c0d0e0f",
            "cursor": null,
            "limit": 101
        }))
        .is_err());
        assert!(ConversationsRequest::parse(serde_json::json!({
            "operation": "create-channel",
            "name": "é".repeat(41)
        }))
        .is_err());
        assert!(ConversationsRequest::parse(serde_json::json!({
            "operation": "create-channel",
            "name": "planning"
        }))
        .is_err());
        assert!(ConversationsRequest::parse(serde_json::json!({
            "operation": "create-channel",
            "name": "#planning"
        }))
        .is_ok());
        assert!(!ConversationChannelView {
            channel_id: "000102030405060708090a0b0c0d0e0f".to_owned(),
            name: "general".to_owned(),
            archived: false,
            unread_count: 0,
            can_manage: false,
        }
        .validate());
        assert!(ConversationsRequest::parse(serde_json::json!({
            "operation": "post-message",
            "channelId": "000102030405060708090a0b0c0d0e0f",
            "markdown": "é".repeat(8_193)
        }))
        .is_err());
    }

    #[test]
    fn finite_error_mapping_hides_lifecycle_and_authorization_details() {
        assert_eq!(
            runtime_error(ConversationRuntimeError::LocalAuthoringBlocked).code,
            ConversationsErrorCode::AuthoringBlocked
        );
        for (reason, code) in [
            (
                "channel is unavailable or archived",
                ConversationsErrorCode::ArchivedChannel,
            ),
            (
                "channel creator authority",
                ConversationsErrorCode::Unauthorized,
            ),
        ] {
            assert_eq!(
                runtime_error(ConversationError::UnauthorizedData(reason).into()).code,
                code
            );
        }
        assert!(ConversationsError::new(ConversationsErrorCode::Internal).validate());
    }

    #[test]
    fn invalidation_serialization_is_secret_free() {
        let value = serde_json::to_value(super::super::workspace::ConversationInvalidationView {
            workspace_id: "workspace".to_owned(),
            channel_id: "channel".to_owned(),
            message_id: Some("message".to_owned()),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "workspaceId": "workspace", "channelId": "channel", "messageId": "message" })
        );
    }
}
