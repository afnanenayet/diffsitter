//! Negative index vector
//!
//! A Python-style negative index vector.

use std::ops::{Index, IndexMut};

/// A vector that can be indexed with a negative index, like with Python.
///
/// ```
/// let v = NegIdxVec::from(vec![1, 2, 3]);
/// let last_negative = v[-1];
/// let last = v[v.0.len() - 1];
/// ```
///
/// A negative index corresponds to an offset from the end of the vector.
#[derive(Debug, Clone)]
pub struct NegIdxVec<T> {
    /// The underlying vector for the negative index vector
    pub data: Vec<T>,

    /// An optional size constraint. Since vectors are dynamically sized, you can define the offset
    /// up front rather than infer it from the vector's size.
    len: usize,
}

#[allow(dead_code)]
impl<T> NegIdxVec<T> {
    /// Create a negative index vector with a given size.
    ///
    /// This will create an internal vector and all offsets will be pegged relative to the size of
    /// this vector.
    ///
    /// ```
    /// let v = NegIdxVec::new(1, Default::default);
    /// ```
    pub fn new<F>(len: usize, f: F) -> Self
    where
        F: FnMut() -> T,
    {
        let mut v = Vec::new();
        v.resize_with(len, f);

        Self { data: v, len }
    }

    /// An internal helper for the indexing methods.
    ///
    /// This will resolve a potentially negative index to the "real" index that can be used
    /// directly with the internal vector.
    ///
    /// If the index is less zero then the index will be transformed by adding `idx` to the offset
    /// so negative indices are relative to the end of the vector.
    fn idx_helper(&self, idx: i32) -> usize {
        let len = self.len;
        let final_index = if idx >= 0 {
            idx as usize
        } else {
            let offset_idx = (len as i32) + idx;
            debug_assert!(offset_idx >= 0);
            offset_idx as usize
        };
        debug_assert!(final_index < len);
        final_index
    }

    /// Get the length of the vector
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<T> From<Vec<T>> for NegIdxVec<T> {
    fn from(v: Vec<T>) -> Self {
        // Need to capture the length before the borrow, and usize is a trivial copy type.
        let len = v.len();
        Self { data: v, len }
    }
}

impl<T> Default for NegIdxVec<T> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            len: 0,
        }
    }
}

impl<T> Index<i32> for NegIdxVec<T> {
    type Output = T;

    fn index(&self, idx: i32) -> &<Self as std::ops::Index<i32>>::Output {
        &self.data[self.idx_helper(idx)]
    }
}

impl<T> IndexMut<i32> for NegIdxVec<T> {
    fn index_mut(&mut self, idx: i32) -> &mut <Self as std::ops::Index<i32>>::Output {
        let offset_idx = self.idx_helper(idx);
        &mut self.data[offset_idx]
    }
}

impl<T> IntoIterator for NegIdxVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a test vector for test cases
    fn test_vector() -> Vec<u32> {
        (0..100).collect()
    }

    #[test]
    fn test_negative_indices() {
        let vec = test_vector();
        let neg_vec = NegIdxVec::<u32>::from(vec.clone());

        for (idx, &elem) in vec.iter().rev().enumerate() {
            assert_eq!(elem, neg_vec[-(idx as i32 + 1)]);
        }
    }

    #[test]
    fn test_positive_indices() {
        let vec = test_vector();
        let neg_vec = NegIdxVec::<u32>::from(vec.clone());

        for (idx, &elem) in vec.iter().enumerate() {
            assert_eq!(elem, neg_vec[idx as i32]);
        }
    }

    #[test]
    #[should_panic]
    fn test_positive_overflow() {
        let vec = NegIdxVec::<u32>::from(test_vector());
        let _ = vec[vec.len() as i32 + 1];
    }

    #[test]
    #[should_panic]
    fn test_negative_overflow() {
        let vec = NegIdxVec::<u32>::from(test_vector());
        let idx = (vec.len() as i32) * -2;
        let _ = vec[idx];
    }
}
