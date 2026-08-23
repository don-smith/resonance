//! Bounded, transport-independent file-history and blob recovery protocol.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    membership_log::MembershipProjection,
    workspace_files::{
        authority::{AuthorityError, WorkspaceFileAuthority},
        blobs::{BlobError, ContentHash},
        FileOperationBody, FileOperationError, SignedFileOperation,
    },
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

pub const FILE_STREAM_ALPN: &[u8] = b"resonance/file-recovery/1";
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 128 * 1024;
pub const MAX_BLOB_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_OPERATION_RECORDS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRequest {
    MissingOperations {
        workspace_id: String,
        known_operation_ids: Vec<String>,
    },
    BlobChunk {
        workspace_id: String,
        content_hash: String,
        offset: u64,
        max_bytes: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileResponse {
    Operations(Vec<Vec<u8>>),
    BlobChunk {
        content_hash: String,
        offset: u64,
        bytes: Vec<u8>,
        complete: bool,
    },
    Denied,
    Invalid,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTransportCodecError {
    Encode,
    Decode,
    TooLarge,
}

#[derive(Debug)]
pub enum FileRecoveryError {
    UnexpectedResponse,
    InvalidChunk,
    FileOperation(FileOperationError),
    Authority(AuthorityError),
    Blob(BlobError),
    Store(WorkspaceStoreError),
}

impl std::fmt::Display for FileRecoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedResponse => formatter.write_str("unexpected file-recovery response"),
            Self::InvalidChunk => formatter.write_str("invalid file-recovery chunk"),
            Self::FileOperation(error) => write!(formatter, "{error}"),
            Self::Authority(error) => write!(formatter, "{error}"),
            Self::Blob(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for FileRecoveryError {}

impl From<FileOperationError> for FileRecoveryError {
    fn from(error: FileOperationError) -> Self {
        Self::FileOperation(error)
    }
}

impl From<AuthorityError> for FileRecoveryError {
    fn from(error: AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<BlobError> for FileRecoveryError {
    fn from(error: BlobError) -> Self {
        Self::Blob(error)
    }
}

impl From<WorkspaceStoreError> for FileRecoveryError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

pub struct FileRecoveryTarget {
    workspace_id: String,
    membership: MembershipProjection,
    authority: WorkspaceFileAuthority,
    durable_operations: BTreeMap<String, Vec<u8>>,
    partial_blobs: BTreeMap<String, Vec<u8>>,
}

impl FileRecoveryTarget {
    #[must_use]
    pub fn new(workspace_id: impl Into<String>, membership: MembershipProjection) -> Self {
        let workspace_id = workspace_id.into();
        Self {
            authority: WorkspaceFileAuthority::new(&workspace_id),
            workspace_id,
            membership,
            durable_operations: BTreeMap::new(),
            partial_blobs: BTreeMap::new(),
        }
    }

    pub(crate) fn open_with_store(
        workspace_id: impl Into<String>,
        membership: MembershipProjection,
        store: &WorkspaceStore,
    ) -> Result<Self, FileRecoveryError> {
        let workspace_id = workspace_id.into();
        let operations = store.file_operations()?;
        let mut authority =
            WorkspaceFileAuthority::with_blob_store(&workspace_id, store.open_blob_store()?);
        if let Err(error) = authority.replay(&operations, &membership) {
            if !matches!(error, AuthorityError::Blob(BlobError::Missing)) {
                return Err(error.into());
            }
        }
        let durable_operations = operations
            .into_iter()
            .map(|operation| {
                Ok((
                    operation.operation.operation_id.clone(),
                    operation.encode()?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, FileRecoveryError>>()?;
        Ok(Self {
            workspace_id,
            membership,
            authority,
            durable_operations,
            partial_blobs: BTreeMap::new(),
        })
    }

    pub fn recover_operations(
        &mut self,
        response: FileResponse,
    ) -> Result<usize, FileRecoveryError> {
        self.recover_operations_with_store(response, None)
    }

    pub fn recover_operations_to_store(
        &mut self,
        response: FileResponse,
        store: &WorkspaceStore,
    ) -> Result<usize, FileRecoveryError> {
        self.recover_operations_with_store(response, Some(store))
    }

    fn recover_operations_with_store(
        &mut self,
        response: FileResponse,
        store: Option<&WorkspaceStore>,
    ) -> Result<usize, FileRecoveryError> {
        let FileResponse::Operations(operations) = response else {
            return Err(FileRecoveryError::UnexpectedResponse);
        };
        if operations.len() > MAX_OPERATION_RECORDS {
            return Err(FileRecoveryError::InvalidChunk);
        }
        let recovered = operations
            .into_iter()
            .map(|bytes| {
                if bytes.len() > MAX_REQUEST_BYTES {
                    return Err(FileRecoveryError::InvalidChunk);
                }
                let operation = SignedFileOperation::decode(&bytes)?;
                if operation_content_hash(&operation).is_some_and(|hash| !valid_content_hash(hash))
                {
                    return Err(FileRecoveryError::InvalidChunk);
                }
                Ok((operation, bytes))
            })
            .collect::<Result<Vec<_>, FileRecoveryError>>()?;
        let mut staged_authority = self.authority.clone();
        let mut staged_operations = self.durable_operations.clone();
        let mut accepted = 0;
        for (operation, bytes) in &recovered {
            if let Err(error) = staged_authority.apply(operation, &self.membership) {
                if !matches!(error, AuthorityError::Blob(BlobError::Missing)) {
                    return Err(error.into());
                }
            }
            if staged_operations
                .insert(operation.operation.operation_id.clone(), bytes.clone())
                .is_none()
            {
                accepted += 1;
            }
        }
        if let Some(store) = store {
            let operations = recovered
                .iter()
                .map(|(operation, _)| operation.clone())
                .collect::<Vec<_>>();
            store.record_file_operations(&operations)?;
        }
        self.durable_operations = staged_operations;
        if let Some(rebuilt) = self.rebuild_authority()? {
            self.authority = rebuilt;
        }
        Ok(accepted)
    }

    #[must_use]
    pub(crate) fn missing_blob_hashes(&self) -> Vec<String> {
        self.durable_operations
            .values()
            .filter_map(|bytes| SignedFileOperation::decode(bytes).ok())
            .filter_map(|operation| operation_content_hash(&operation).map(ToOwned::to_owned))
            .filter(|hash| {
                !self
                    .authority
                    .blob_store()
                    .contains(&ContentHash(hash.clone()))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[must_use]
    pub fn missing_operations_request(&self) -> FileRequest {
        FileRequest::MissingOperations {
            workspace_id: self.workspace_id.clone(),
            known_operation_ids: self.durable_operations.keys().cloned().collect(),
        }
    }

    #[must_use]
    pub fn next_blob_request(&self, content_hash: &str) -> FileRequest {
        FileRequest::BlobChunk {
            workspace_id: self.workspace_id.clone(),
            content_hash: content_hash.to_owned(),
            offset: self
                .partial_blobs
                .get(content_hash)
                .map_or(0, |bytes| bytes.len() as u64),
            max_bytes: MAX_BLOB_CHUNK_BYTES as u32,
        }
    }

    pub fn recover_blob_chunk(
        &mut self,
        response: FileResponse,
    ) -> Result<bool, FileRecoveryError> {
        let FileResponse::BlobChunk {
            content_hash,
            offset,
            bytes,
            complete,
        } = response
        else {
            return Err(FileRecoveryError::UnexpectedResponse);
        };
        if !valid_content_hash(&content_hash) || bytes.len() > MAX_BLOB_CHUNK_BYTES {
            return Err(FileRecoveryError::InvalidChunk);
        }
        let partial = self.partial_blobs.entry(content_hash.clone()).or_default();
        if offset != partial.len() as u64 || (bytes.is_empty() && !complete) {
            return Err(FileRecoveryError::InvalidChunk);
        }
        partial.extend(bytes);
        if !complete {
            return Ok(false);
        }
        let completed = self
            .partial_blobs
            .remove(&content_hash)
            .expect("completed partial was inserted");
        self.authority
            .blob_store_mut()
            .store_verified(&ContentHash(content_hash), &completed)?;
        if let Some(rebuilt) = self.rebuild_authority()? {
            self.authority = rebuilt;
        }
        Ok(true)
    }

    fn rebuild_authority(&self) -> Result<Option<WorkspaceFileAuthority>, FileRecoveryError> {
        let mut authority = WorkspaceFileAuthority::with_blob_store(
            &self.workspace_id,
            self.authority.blob_store().clone(),
        );
        let operations = self
            .durable_operations
            .values()
            .map(|bytes| SignedFileOperation::decode(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        match authority.replay(&operations, &self.membership) {
            Ok(()) => Ok(Some(authority)),
            Err(AuthorityError::Blob(BlobError::Missing)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    #[must_use]
    pub fn authority(&self) -> &WorkspaceFileAuthority {
        &self.authority
    }

    #[must_use]
    pub fn durable_operations(&self) -> Vec<Vec<u8>> {
        self.durable_operations.values().cloned().collect()
    }

    pub fn replay_durable(&mut self, operations: Vec<Vec<u8>>) -> Result<(), FileRecoveryError> {
        for bytes in operations {
            self.recover_operations(FileResponse::Operations(vec![bytes]))?;
        }
        Ok(())
    }
}

fn operation_content_hash(operation: &SignedFileOperation) -> Option<&str> {
    match &operation.operation.body {
        FileOperationBody::CreateFile { content_hash, .. }
        | FileOperationBody::ReplaceFileRevision { content_hash, .. } => Some(content_hash),
        _ => None,
    }
}

fn valid_operation_id(value: &str) -> bool {
    valid_lower_hex(value, 32)
}

fn valid_content_hash(value: &str) -> bool {
    valid_lower_hex(value, 64)
}

fn valid_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Default)]
pub struct FileRecoveryService {
    workspace_id: String,
    members: BTreeSet<String>,
    operations: BTreeMap<String, Vec<u8>>,
    blobs: BTreeMap<String, Vec<u8>>,
}

impl FileRecoveryService {
    #[must_use]
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            ..Self::default()
        }
    }

    pub fn set_members(&mut self, members: impl IntoIterator<Item = String>) {
        self.members = members.into_iter().collect();
    }

    pub fn insert_operation(&mut self, operation_id: impl Into<String>, bytes: Vec<u8>) {
        self.operations.entry(operation_id.into()).or_insert(bytes);
    }

    pub fn insert_blob(&mut self, bytes: Vec<u8>) -> String {
        let hash = ContentHash::from_bytes(&bytes).as_str().to_owned();
        self.blobs.entry(hash.clone()).or_insert(bytes);
        hash
    }

    pub fn handle(&self, remote_member: &str, request: FileRequest) -> FileResponse {
        if !self.members.contains(remote_member) {
            return FileResponse::Denied;
        }
        match request {
            FileRequest::MissingOperations {
                workspace_id,
                known_operation_ids,
            } => {
                if workspace_id != self.workspace_id
                    || known_operation_ids.len() > MAX_OPERATION_RECORDS
                    || known_operation_ids.iter().any(|id| !valid_operation_id(id))
                {
                    return FileResponse::Invalid;
                }
                let known = known_operation_ids.into_iter().collect::<BTreeSet<_>>();
                let mut total = 1024_usize;
                let operations = self
                    .operations
                    .iter()
                    .filter(|(id, _)| !known.contains(*id))
                    .take(MAX_OPERATION_RECORDS)
                    .take_while(|(_, bytes)| {
                        total = total.saturating_add(bytes.len());
                        total <= MAX_RESPONSE_BYTES
                    })
                    .map(|(_, bytes)| bytes.clone())
                    .collect();
                FileResponse::Operations(operations)
            }
            FileRequest::BlobChunk {
                workspace_id,
                content_hash,
                offset,
                max_bytes,
            } => {
                if workspace_id != self.workspace_id
                    || !valid_content_hash(&content_hash)
                    || max_bytes == 0
                    || max_bytes as usize > MAX_BLOB_CHUNK_BYTES
                {
                    return FileResponse::Invalid;
                }
                let Some(blob) = self.blobs.get(&content_hash) else {
                    return FileResponse::Missing;
                };
                let Ok(offset) = usize::try_from(offset) else {
                    return FileResponse::Invalid;
                };
                if offset > blob.len() {
                    return FileResponse::Invalid;
                }
                let end = offset.saturating_add(max_bytes as usize).min(blob.len());
                FileResponse::BlobChunk {
                    content_hash,
                    offset: offset as u64,
                    bytes: blob[offset..end].to_vec(),
                    complete: end == blob.len(),
                }
            }
        }
    }
}

pub fn encode_request(request: &FileRequest) -> Result<Vec<u8>, FileTransportCodecError> {
    let bytes = postcard::to_stdvec(request).map_err(|_| FileTransportCodecError::Encode)?;
    (bytes.len() <= MAX_REQUEST_BYTES)
        .then_some(bytes)
        .ok_or(FileTransportCodecError::TooLarge)
}

pub fn decode_request(bytes: &[u8]) -> Result<FileRequest, FileTransportCodecError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(FileTransportCodecError::TooLarge);
    }
    postcard::from_bytes(bytes).map_err(|_| FileTransportCodecError::Decode)
}

pub fn encode_response(response: &FileResponse) -> Result<Vec<u8>, FileTransportCodecError> {
    let bytes = postcard::to_stdvec(response).map_err(|_| FileTransportCodecError::Encode)?;
    (bytes.len() <= MAX_RESPONSE_BYTES)
        .then_some(bytes)
        .ok_or(FileTransportCodecError::TooLarge)
}

pub fn decode_response(bytes: &[u8]) -> Result<FileResponse, FileTransportCodecError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(FileTransportCodecError::TooLarge);
    }
    postcard::from_bytes(bytes).map_err(|_| FileTransportCodecError::Decode)
}
