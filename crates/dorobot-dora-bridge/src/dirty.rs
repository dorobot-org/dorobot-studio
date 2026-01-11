//! Dirty tracking primitives for efficient UI polling
//!
//! These types allow producers (Dora nodes) to push data and consumers (Makepad UI)
//! to efficiently check if new data is available without unnecessary processing.

use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// A single value with dirty tracking
///
/// Producers call `set()` to update the value and mark it dirty.
/// Consumers call `read_if_dirty()` to get the value only if it changed.
pub struct DirtyValue<T> {
    value: RwLock<T>,
    dirty: AtomicBool,
}

impl<T: Default> Default for DirtyValue<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> DirtyValue<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: RwLock::new(value),
            dirty: AtomicBool::new(false),
        }
    }

    /// Set a new value and mark as dirty
    pub fn set(&self, value: T) {
        *self.value.write() = value;
        self.dirty.store(true, Ordering::Release);
    }

    /// Check if dirty without reading
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Read the value only if dirty, clearing the dirty flag
    pub fn read_if_dirty(&self) -> Option<T>
    where
        T: Clone,
    {
        if self.dirty.swap(false, Ordering::AcqRel) {
            Some(self.value.read().clone())
        } else {
            None
        }
    }

    /// Read the current value regardless of dirty state
    pub fn read(&self) -> T
    where
        T: Clone,
    {
        self.value.read().clone()
    }

    /// Get a reference to the value (holds read lock)
    pub fn get(&self) -> parking_lot::RwLockReadGuard<T> {
        self.value.read()
    }

    /// Modify the value in place and mark as dirty
    pub fn modify<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(&mut *self.value.write());
        self.dirty.store(true, Ordering::Release);
    }
}

/// A vector with dirty tracking and optional size limit
///
/// Producers call `push()` to add items.
/// Consumers call `drain_if_dirty()` to get all items and clear the buffer.
pub struct DirtyVec<T> {
    items: RwLock<Vec<T>>,
    dirty: AtomicBool,
    max_size: usize,
}

impl<T> Default for DirtyVec<T> {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl<T> DirtyVec<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            items: RwLock::new(Vec::new()),
            dirty: AtomicBool::new(false),
            max_size,
        }
    }

    /// Push an item to the buffer
    pub fn push(&self, item: T) {
        let mut items = self.items.write();
        if items.len() >= self.max_size {
            items.remove(0);
        }
        items.push(item);
        self.dirty.store(true, Ordering::Release);
    }

    /// Push multiple items
    pub fn extend(&self, new_items: impl IntoIterator<Item = T>) {
        let mut items = self.items.write();
        for item in new_items {
            if items.len() >= self.max_size {
                items.remove(0);
            }
            items.push(item);
        }
        self.dirty.store(true, Ordering::Release);
    }

    /// Check if dirty without reading
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Drain all items if dirty, clearing the dirty flag
    pub fn drain_if_dirty(&self) -> Option<Vec<T>> {
        if self.dirty.swap(false, Ordering::AcqRel) {
            let mut items = self.items.write();
            Some(std::mem::take(&mut *items))
        } else {
            None
        }
    }

    /// Read items without draining (clones)
    pub fn read_if_dirty(&self) -> Option<Vec<T>>
    where
        T: Clone,
    {
        if self.dirty.swap(false, Ordering::AcqRel) {
            Some(self.items.read().clone())
        } else {
            None
        }
    }

    /// Get current length
    pub fn len(&self) -> usize {
        self.items.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.items.read().is_empty()
    }

    /// Clear all items
    pub fn clear(&self) {
        self.items.write().clear();
        self.dirty.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_value() {
        let value = DirtyValue::new(42);

        // Initial read clears dirty flag set during new()
        assert!(!value.is_dirty());

        // Set marks as dirty
        value.set(100);
        assert!(value.is_dirty());

        // Read if dirty returns value and clears flag
        assert_eq!(value.read_if_dirty(), Some(100));
        assert!(!value.is_dirty());

        // Second read returns None
        assert_eq!(value.read_if_dirty(), None);
    }

    #[test]
    fn test_dirty_vec() {
        let vec: DirtyVec<i32> = DirtyVec::new(5);

        vec.push(1);
        vec.push(2);
        vec.push(3);

        assert!(vec.is_dirty());
        assert_eq!(vec.drain_if_dirty(), Some(vec![1, 2, 3]));
        assert!(!vec.is_dirty());
        assert!(vec.is_empty());
    }

    #[test]
    fn test_dirty_vec_max_size() {
        let vec: DirtyVec<i32> = DirtyVec::new(3);

        vec.extend([1, 2, 3, 4, 5]);

        // Should only keep last 3
        let items = vec.drain_if_dirty().unwrap();
        assert_eq!(items, vec![3, 4, 5]);
    }
}
