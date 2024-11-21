use anyhow::Context;
use axum::async_trait;
use c8y_api::http_proxy::C8yAuthRetriever;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SharedTokenManager(pub(crate) Arc<Mutex<dyn TokenManager>>);

impl SharedTokenManager {
    /// Returns a JWT that doesn't match the provided JWT
    ///
    /// This prevents needless token refreshes if multiple requests are made in parallel
    pub async fn not_matching(&self, input: Option<&Arc<str>>) -> Result<Arc<str>, anyhow::Error> {
        self.0.lock().await.not_matching(input).await
    }
}

#[async_trait]
pub(crate) trait TokenManager: Send {
    async fn not_matching(&mut self, input: Option<&Arc<str>>) -> Result<Arc<str>, anyhow::Error> {
        match (self.cached_mut(), input) {
            (Some(token), None) => Ok(token.clone()),
            // The token should have arisen from this TokenManager, so pointer equality is sufficient
            (Some(token), Some(no_match)) if !Arc::ptr_eq(token, no_match) => Ok(token.clone()),
            _ => self.refresh().await,
        }
    }

    async fn refresh(&mut self) -> Result<Arc<str>, anyhow::Error>;

    fn cached_mut(&mut self) -> Option<&mut Arc<str>>;
}

pub struct C8yTokenManager {
    auth_retriever: C8yAuthRetriever,
    cached: Option<Arc<str>>,
}

impl C8yTokenManager {
    pub fn new(auth_retriever: C8yAuthRetriever) -> Self {
        Self {
            auth_retriever,
            cached: None,
        }
    }

    pub fn shared(self) -> SharedTokenManager {
        SharedTokenManager(Arc::new(Mutex::new(self)))
    }
}

#[async_trait]
impl TokenManager for C8yTokenManager {
    async fn refresh(&mut self) -> Result<Arc<str>, anyhow::Error> {
        let auth = self
            .auth_retriever
            .get_auth_header_value()
            .await
            .context("Authorization is missing from header")?;
        self.cached = Some(auth.to_str()?.into());
        Ok(self.cached.as_ref().unwrap().clone())
    }

    fn cached_mut(&mut self) -> Option<&mut Arc<str>> {
        self.cached.as_mut()
    }
}
