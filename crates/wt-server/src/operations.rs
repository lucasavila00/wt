use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex};
use thiserror::Error;
use wt_api::InstanceName;

type OperationKey = (String, InstanceName);
type OperationState = (Mutex<HashSet<OperationKey>>, Condvar);

#[derive(Clone, Debug, Default)]
pub struct Operations {
    state: Arc<OperationState>,
}

#[derive(Debug)]
pub struct OperationGuard {
    state: Arc<OperationState>,
    key: Option<OperationKey>,
}

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("operation coordinator lock is poisoned")]
    Poisoned,
    #[error("world operation is active")]
    Active,
}

impl Operations {
    pub fn lock(&self, owner: &str, name: &InstanceName) -> Result<OperationGuard, OperationError> {
        let key = (owner.to_owned(), name.clone());
        let (active, wake) = &*self.state;
        let mut active = active.lock().map_err(|_| OperationError::Poisoned)?;
        while active.contains(&key) {
            active = wake.wait(active).map_err(|_| OperationError::Poisoned)?;
        }
        active.insert(key.clone());
        Ok(OperationGuard {
            state: Arc::clone(&self.state),
            key: Some(key),
        })
    }

    pub fn try_lock(
        &self,
        owner: &str,
        name: &InstanceName,
    ) -> Result<OperationGuard, OperationError> {
        let key = (owner.to_owned(), name.clone());
        let (active, _) = &*self.state;
        let mut active = active.lock().map_err(|_| OperationError::Poisoned)?;
        if !active.insert(key.clone()) {
            return Err(OperationError::Active);
        }
        Ok(OperationGuard {
            state: Arc::clone(&self.state),
            key: Some(key),
        })
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };
        let (active, wake) = &*self.state;
        if let Ok(mut active) = active.lock() {
            active.remove(&key);
            wake.notify_all();
        }
    }
}
