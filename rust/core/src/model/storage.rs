use crate::model::base::AppResult;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin; // Ensure AppError is accessible via AppResult or directly

// We might want StorageError specific error later, but for now AppResult implies AppError.
// But Stream Item needs to be Result<Bytes, E>.
// Let's use generic error for stream or AppError.

pub struct ByteStream(Pin<Box<dyn Stream<Item = AppResult<Bytes>> + Send>>);

impl ByteStream {
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = AppResult<Bytes>> + Send + 'static,
    {
        Self(Box::pin(stream))
    }

    pub fn into_inner(self) -> Pin<Box<dyn Stream<Item = AppResult<Bytes>> + Send>> {
        self.0
    }
}

impl Stream for ByteStream {
    type Item = AppResult<Bytes>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
}

impl From<Bytes> for ByteStream {
    fn from(bytes: Bytes) -> Self {
        Self::new(futures::stream::once(async move { Ok(bytes) }))
    }
}

impl From<Vec<u8>> for ByteStream {
    fn from(vec: Vec<u8>) -> Self {
        Bytes::from(vec).into()
    }
}

impl From<String> for ByteStream {
    fn from(s: String) -> Self {
        Bytes::from(s).into()
    }
}

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub destination: crate::model::file::FileLocation,
    pub content_type: Option<String>,
    pub size: Option<u64>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_byte_stream_from_bytes() {
        let content = Bytes::from("hello world");
        let mut stream = ByteStream::from(content.clone());
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result, content);
    }

    #[tokio::test]
    async fn test_byte_stream_from_vec() {
        let content = vec![1, 2, 3];
        let mut stream = ByteStream::from(content.clone());
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result, Bytes::from(content));
    }

    #[tokio::test]
    async fn test_byte_stream_from_string() {
        let content = "hello".to_string();
        let mut stream = ByteStream::from(content.clone());
        let result = stream.next().await.unwrap().unwrap();
        assert_eq!(result, Bytes::from(content));
    }
}
