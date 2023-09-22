use std::sync::Arc;

use c8y_http_proxy::credentials::JwtRetriever;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SharedTokenManager(Arc<Mutex<TokenManager>>);

impl SharedTokenManager {
    /// Returns a JWT that doesn't match the provided JWT
    ///
    /// This prevents needless token refreshes if multiple requests are made in parallel
    pub async fn not_matching(&self, input: Option<&Arc<str>>) -> Arc<str> {
        self.0.lock().await.not_matching(input).await
    }
}

pub struct TokenManager {
    recv: JwtRetriever,
    cached: Option<Arc<str>>,
}

impl TokenManager {
    pub fn new(recv: JwtRetriever) -> Self {
        Self { recv, cached: None }
    }

    pub fn shared(self) -> SharedTokenManager {
        SharedTokenManager(Arc::new(Mutex::new(self)))
    }
}

impl TokenManager {
    async fn not_matching(&mut self, input: Option<&Arc<str>>) -> Arc<str> {
        match (self.cached.as_mut(), input) {
            (Some(token), None) => token.clone(),
            // The token should have arisen from this TokenManager, so pointer equality is sufficient
            (Some(token), Some(no_match)) if !Arc::ptr_eq(token, no_match) => token.clone(),
            _ => self.refresh().await,
        }
    }

    async fn refresh(&mut self) -> Arc<str> {
        self.cached = Some(self.recv.await_response(()).await.unwrap().unwrap().into());
        self.cached.as_ref().unwrap().clone()
    }
}
