use std::sync::Arc;

use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use tokio::sync::RwLock;

use crate::workspace_file_transport::{
    decode_request, encode_response, FileRecoveryService, FileResponse, MAX_REQUEST_BYTES,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct FileStreamHandler {
    service: Arc<RwLock<FileRecoveryService>>,
}

impl FileStreamHandler {
    pub(crate) async fn replace_service(&self, service: FileRecoveryService) {
        *self.service.write().await = service;
    }
}

impl ProtocolHandler for FileStreamHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote = connection.remote_id().to_string();
        let (mut send, mut receive) = connection.accept_bi().await?;
        let response = match receive.read_to_end(MAX_REQUEST_BYTES).await {
            Ok(bytes) => match decode_request(&bytes) {
                Ok(request) => self.service.read().await.handle(&remote, request),
                Err(_) => FileResponse::Invalid,
            },
            Err(_) => FileResponse::Invalid,
        };
        if let Ok(bytes) = encode_response(&response) {
            send.write_all(&bytes)
                .await
                .map_err(std::io::Error::other)?;
            send.finish()?;
            connection.closed().await;
        }
        Ok(())
    }
}
