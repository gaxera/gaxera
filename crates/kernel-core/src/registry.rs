use crate::object::ObjectId;
use alloc::collections::BTreeMap;

/// A common abstraction for typed object registries.
///
/// Registries store the actual concrete state machines (e.g., `Endpoint`, `MemoryObject`)
/// associated with an `ObjectId`. The identity itself (and its lifecycle) is managed
/// by the `ObjectArena`.
pub trait ObjectRegistry<T> {
    /// Inserts a new object into the registry.
    fn insert(&mut self, id: ObjectId, object: T);

    /// Retrieves a reference to an object, verifying its exact generation.
    fn get(&self, id: ObjectId) -> Option<&T>;

    /// Retrieves a mutable reference to an object, verifying its exact generation.
    fn get_mut(&mut self, id: ObjectId) -> Option<&mut T>;

    /// Removes an object from the registry, yielding ownership so its `Drop`
    /// implementation can reclaim hardware resources.
    fn remove(&mut self, id: ObjectId) -> Option<T>;
}

/// A BTreeMap-backed object registry suitable for M7.
pub struct BTreeRegistry<T> {
    store: BTreeMap<u32, (u32, T)>, // Map index -> (generation, Object)
}

impl<T> BTreeRegistry<T> {
    pub const fn new() -> Self {
        Self {
            store: BTreeMap::new(),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ObjectId, &mut T)> {
        self.store.iter_mut().map(|(idx, (generation, obj))| {
            (
                ObjectId::from_raw(((*generation as u64) << 32) | (*idx as u64)),
                obj,
            )
        })
    }
}

impl<T> Default for BTreeRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ObjectRegistry<T> for BTreeRegistry<T> {
    fn insert(&mut self, id: ObjectId, object: T) {
        self.store.insert(id.index(), (id.generation(), object));
    }

    fn get(&self, id: ObjectId) -> Option<&T> {
        self.store.get(&id.index()).and_then(|(generation, obj)| {
            if *generation == id.generation() {
                Some(obj)
            } else {
                None
            }
        })
    }

    fn get_mut(&mut self, id: ObjectId) -> Option<&mut T> {
        self.store
            .get_mut(&id.index())
            .and_then(|(generation, obj)| {
                if *generation == id.generation() {
                    Some(obj)
                } else {
                    None
                }
            })
    }

    fn remove(&mut self, id: ObjectId) -> Option<T> {
        // We only remove if the generation matches, to prevent a stale handle
        // from deleting a re-allocated slot.
        if let Some((generation, _)) = self.store.get(&id.index())
            && *generation == id.generation()
        {
            return self.store.remove(&id.index()).map(|(_, obj)| obj);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct DummyObject {
        val: u32,
    }

    #[test]
    fn test_registry_insert_get() {
        let mut registry = BTreeRegistry::new();
        let id = ObjectId::new_for_test(1, 1);

        registry.insert(id, DummyObject { val: 42 });
        assert_eq!(registry.get(id), Some(&DummyObject { val: 42 }));

        // Stale generation should fail
        let stale_id = ObjectId::new_for_test(1, 0);
        assert_eq!(registry.get(stale_id), None);

        // Future generation should fail
        let future_id = ObjectId::new_for_test(1, 2);
        assert_eq!(registry.get(future_id), None);
    }

    #[test]
    fn test_registry_remove() {
        let mut registry = BTreeRegistry::new();
        let id1 = ObjectId::new_for_test(1, 1);

        registry.insert(id1, DummyObject { val: 42 });

        // Removing with wrong generation fails
        assert_eq!(registry.remove(ObjectId::new_for_test(1, 2)), None);
        assert_eq!(registry.get(id1), Some(&DummyObject { val: 42 }));

        // Correct generation succeeds
        assert_eq!(registry.remove(id1), Some(DummyObject { val: 42 }));
        assert_eq!(registry.get(id1), None);
    }

    #[test]
    fn test_registry_get_mut() {
        let mut registry = BTreeRegistry::new();
        let id = ObjectId::new_for_test(1, 1);

        registry.insert(id, DummyObject { val: 42 });

        if let Some(obj) = registry.get_mut(id) {
            obj.val = 100;
        }

        assert_eq!(registry.get(id), Some(&DummyObject { val: 100 }));
    }
}
