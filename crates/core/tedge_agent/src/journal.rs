use crate::state::{State, StateStatus};
use agent_interface::request::AgentRequest;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use rustbreak::deser::Ron;
use rustbreak::PathDatabase;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// The journal of pending operations
///
/// This journal contains all the pending operations
/// and is written on disk each time an operation is:
/// - scheduled   (the new operation is added)
/// - launched    (the operation status is updated)
/// - completed   (the operation is removed)
/// - cancelled   (the operation is removed)
#[derive(Clone)]
pub struct Journal {
    db: Arc<Mutex<JournalDB>>,
}

impl Journal {
    pub async fn open(path: PathBuf) -> Result<Journal, JournalError> {
        let db = JournalDB::open(path).await?;
        Ok(Journal {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub async fn schedule(&self, request: AgentRequest) -> Result<(), JournalError> {
        let mut db = self.db.lock().await;
        db.schedule(request).await
    }

    pub async fn next_request(&self) -> Option<AgentRequest> {
        let mut db = self.db.lock().await;
        db.next_request().await
    }
}

pub struct JournalDB {
    db: PathDatabase<JournalData, Ron>,
    request_sender: mpsc::UnboundedSender<AgentRequest>,
    request_receiver: mpsc::UnboundedReceiver<AgentRequest>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
struct JournalData {
    state: State,
    pending: VecDeque<AgentRequest>,
}

#[derive(thiserror::Error, Debug)]
pub enum JournalError {
    #[error("Agent database error: {0}")]
    RustbreakError(#[from] rustbreak::error::RustbreakError),
}

impl JournalDB {
    pub async fn open(path: PathBuf) -> Result<JournalDB, JournalError> {
        let empty = JournalData::default();
        let db = PathDatabase::<JournalData, Ron>::load_from_path_or(path, empty)?;
        let (mut request_sender, request_receiver) = mpsc::unbounded();

        let requests: Vec<AgentRequest> = {
            // Extract the requests, to force the data `Lock` to stay in a scope with no `.await`.
            let data = db.borrow_data()?;
            data.pending.iter().map(|r| r.clone()).collect()
        };

        for request in requests.into_iter() {
            let _ = request_sender.send(request).await;
        }

        Ok(JournalDB {
            db,
            request_sender,
            request_receiver,
        })
    }

    pub async fn schedule(&mut self, request: AgentRequest) -> Result<(), JournalError> {
        {
            let mut data = self.db.borrow_data_mut()?;
            data.pending.push_back(request.clone());
        }
        self.db.save()?;

        let _ = self.request_sender.send(request).await;
        Ok(())
    }

    pub async fn next_request(&mut self) -> Option<AgentRequest> {
        self.request_receiver.next().await
    }
}

#[async_trait::async_trait]
impl crate::state::StateRepository for Journal {
    type Error = JournalError;

    async fn load(&self) -> Result<State, Self::Error> {
        let db = self.db.lock().await;
        db.load().await
    }

    async fn store(&self, state: &State) -> Result<(), Self::Error> {
        let db = self.db.lock().await;
        db.store(state).await
    }

    async fn clear(&self) -> Result<State, Self::Error> {
        let db = self.db.lock().await;
        db.clear().await
    }

    async fn update(&self, status: &StateStatus) -> Result<(), Self::Error> {
        let db = self.db.lock().await;
        db.update(status).await
    }
}

#[async_trait::async_trait]
impl crate::state::StateRepository for JournalDB {
    type Error = JournalError;

    async fn load(&self) -> Result<State, Self::Error> {
        let data = self.db.borrow_data()?;
        Ok(data.state.clone())
    }

    async fn store(&self, state: &State) -> Result<(), Self::Error> {
        {
            let mut data = self.db.borrow_data_mut()?;
            data.state = state.clone();
        }
        Ok(self.db.save()?)
    }

    async fn clear(&self) -> Result<State, Self::Error> {
        let state = State::default();
        self.store(&state).await?;
        Ok(state)
    }

    async fn update(&self, status: &StateStatus) -> Result<(), Self::Error> {
        {
            let mut data = self.db.borrow_data_mut()?;
            data.state.operation = Some(status.to_owned());
        }
        Ok(self.db.save()?)
    }
}
