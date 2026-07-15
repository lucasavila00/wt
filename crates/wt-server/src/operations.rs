use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
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

impl Operations {
    pub fn lock(&self, owner: &str, name: &InstanceName) -> OperationGuard {
        let key = (owner.to_owned(), name.clone());
        let (active, wake) = &*self.state;
        let mut active = lock_unpoisoned(active);
        while active.contains(&key) {
            active = wake.wait(active).unwrap_or_else(|error| error.into_inner());
        }
        active.insert(key.clone());
        OperationGuard {
            state: Arc::clone(&self.state),
            key: Some(key),
        }
    }

    pub fn try_lock(&self, owner: &str, name: &InstanceName) -> Option<OperationGuard> {
        let key = (owner.to_owned(), name.clone());
        let (active, _) = &*self.state;
        let mut active = lock_unpoisoned(active);
        if !active.insert(key.clone()) {
            return None;
        }
        Some(OperationGuard {
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
        lock_unpoisoned(active).remove(&key);
        wake.notify_all();
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
