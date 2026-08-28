use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use wt_control_protocol::{WorldId, WorldName};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum OperationKey {
    Create(WorldName),
    World(WorldId),
}
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

#[derive(Debug)]
pub struct WorldOperationGuard {
    _operation: OperationGuard,
    world_id: WorldId,
}

impl Operations {
    pub fn lock_create(&self, name: &WorldName) -> OperationGuard {
        let key = OperationKey::Create(name.clone());
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

    pub fn try_lock_world(&self, world_id: WorldId) -> Option<WorldOperationGuard> {
        let key = OperationKey::World(world_id);
        let (active, _) = &*self.state;
        let mut active = lock_unpoisoned(active);
        if !active.insert(key.clone()) {
            return None;
        }
        Some(WorldOperationGuard {
            _operation: OperationGuard {
                state: Arc::clone(&self.state),
                key: Some(key),
            },
            world_id,
        })
    }
}

impl WorldOperationGuard {
    pub(crate) fn world_id(&self) -> WorldId {
        self.world_id
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
