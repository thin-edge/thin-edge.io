use crate::state::{State, StateStatus};
use rustbreak::deser::Ron;
use rustbreak::PathDatabase;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Journal {
    db: PathDatabase<JournalData, Ron>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct JournalData {
    pub state: State,
}

#[derive(thiserror::Error, Debug)]
pub enum JournalError {
    #[error("Agent database error: {0}")]
    RustbreakError(#[from] rustbreak::error::RustbreakError),
}

impl Journal {
    pub fn open(path: PathBuf) -> Result<Journal, JournalError> {
        let empty = JournalData::default();
        let db = PathDatabase::<JournalData, Ron>::load_from_path_or(path, empty)?;

        Ok(Journal { db })
    }
}

#[async_trait::async_trait]
impl crate::state::StateRepository for Journal {
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
