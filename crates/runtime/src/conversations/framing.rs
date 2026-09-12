//! Bounded framing carried inside one Commonware application message.

use super::{wire::MAX_FRAME_BYTES, ConversationError};

const LENGTH_BYTES: usize = 4;

pub(crate) fn encode(exact: &[u8]) -> Result<Vec<u8>, ConversationError> {
    let frame_len = LENGTH_BYTES
        .checked_add(exact.len())
        .ok_or(ConversationError::SizeLimit {
            field: "frame bytes",
            limit: MAX_FRAME_BYTES,
        })?;
    if frame_len > MAX_FRAME_BYTES {
        return Err(ConversationError::SizeLimit {
            field: "frame bytes",
            limit: MAX_FRAME_BYTES,
        });
    }
    let length = u32::try_from(exact.len()).map_err(|_| ConversationError::SizeLimit {
        field: "frame bytes",
        limit: MAX_FRAME_BYTES,
    })?;
    let mut frame = Vec::with_capacity(frame_len);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(exact);
    Ok(frame)
}

pub(crate) fn decode(frame: &[u8]) -> Result<&[u8], ConversationError> {
    if frame.len() < LENGTH_BYTES || frame.len() > MAX_FRAME_BYTES {
        return Err(ConversationError::MalformedBytes("invalid frame extent"));
    }
    let declared = u32::from_be_bytes(
        frame[..LENGTH_BYTES]
            .try_into()
            .map_err(|_| ConversationError::MalformedBytes("frame length"))?,
    ) as usize;
    if declared != frame.len() - LENGTH_BYTES {
        return Err(ConversationError::MalformedBytes("frame length mismatch"));
    }
    Ok(&frame[LENGTH_BYTES..])
}
