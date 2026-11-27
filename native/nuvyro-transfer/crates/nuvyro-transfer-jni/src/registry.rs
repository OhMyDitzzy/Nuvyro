use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::contract::{ABI_VERSION, NativeStatus};
use crate::engine::Engine;

const MAX_ENGINES: usize = 4;
struct Registry {
    next: i64,
    engines: BTreeMap<i64, Arc<Engine>>,
}
impl Registry {
    fn new() -> Self {
        Self {
            next: 1,
            engines: BTreeMap::new(),
        }
    }
    fn create(&mut self, abi: i32) -> Result<i64, NativeStatus> {
        if abi != ABI_VERSION {
            return Err(NativeStatus::AbiMismatch);
        }
        if self.engines.len() >= MAX_ENGINES {
            return Err(NativeStatus::ResourceLimit);
        }
        let handle = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(NativeStatus::ResourceLimit)?;
        self.engines.insert(handle, Arc::new(Engine::new()));
        Ok(handle)
    }
    fn remove(&mut self, handle: i64) -> Result<Arc<Engine>, NativeStatus> {
        self.engines
            .remove(&handle)
            .ok_or(NativeStatus::InvalidHandle)
    }
}
fn registry() -> &'static Mutex<Registry> {
    static VALUE: OnceLock<Mutex<Registry>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(Registry::new()))
}
pub(crate) fn create(abi: i32) -> Result<i64, NativeStatus> {
    registry()
        .lock()
        .map_err(|_| NativeStatus::InternalFailure)?
        .create(abi)
}
pub(crate) fn get(handle: i64) -> Result<Arc<Engine>, NativeStatus> {
    registry()
        .lock()
        .map_err(|_| NativeStatus::InternalFailure)?
        .engines
        .get(&handle)
        .cloned()
        .ok_or(NativeStatus::InvalidHandle)
}
pub(crate) fn destroy(handle: i64) -> Result<(), NativeStatus> {
    let engine = registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(handle)?;
    engine.close();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mismatch_double_destroy_and_handle_reuse() {
        let mut registry = Registry::new();
        assert_eq!(registry.create(0), Err(NativeStatus::AbiMismatch));
        let first = registry.create(ABI_VERSION).unwrap();
        registry.remove(first).unwrap().close();
        assert!(matches!(
            registry.remove(first),
            Err(NativeStatus::InvalidHandle)
        ));
        let second = registry.create(ABI_VERSION).unwrap();
        assert!(second > first);
        for _ in 1..MAX_ENGINES {
            registry.create(ABI_VERSION).unwrap();
        }
        assert_eq!(
            registry.create(ABI_VERSION),
            Err(NativeStatus::ResourceLimit)
        );
    }
}
