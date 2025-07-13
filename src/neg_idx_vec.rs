//! Negative index vector
//!
//! A Python-style negative index vector.

use std::ops::{Index, IndexMut};

/// A vector that can be indexed with a negative index, like with Python.
///
/// ```rust
/// use libdiffsitter::neg_idx_vec::NegIdxVec;
/// let v = NegIdxVec::from(vec![1, 2, 3]);
/// let last_negative = v[-1];
/// let last = v[(v.len() - 1).try_into().unwrap()];
/// ```
///
/// A negative index corresponds to an offset from the end of the vector.
#[derive(Debug, Clone, Eq, PartialEq)]
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
    /// ```rust
    /// use libdiffsitter::neg_idx_vec::NegIdxVec;
    /// let v: NegIdxVec<usize> = NegIdxVec::new(1, Default::default);
    /// ```
    pub fn new<F>(len: usize, f: F) -> Self
    where
        F: FnMut() -> T,
    {
        let mut v = Vec::new();
        v.resize_with(len, f);

        Self { data: v, len }
    }

    /// Reserve capacity for a number of *additional* elements.
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }

    /// Reserve space for exactly `additional` elements.
    ///
    /// This will not over-allocate.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.data.reserve_exact(additional);
    }

    /// Return the total number of elements the vector can hold without requiring another
    /// allocation.
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// An internal helper for the indexing methods.
    ///
    /// This will resolve a potentially negative index to the "real" index that can be used
    /// directly with the internal vector.
    ///
    /// If the index is less zero then the index will be transformed by adding `idx` to the offset
    /// so negative indices are relative to the end of the vector.
    fn idx_helper(&self, idx: i32) -> usize {
        let len: i32 = self.len.try_into().unwrap();

        let final_index = if idx >= 0 {
            idx.try_into().unwrap()
        } else {
            let offset_idx = len + idx;
            debug_assert!(offset_idx >= 0);
            offset_idx.try_into().unwrap()
        };
        debug_assert!(final_index < len.try_into().unwrap());
        final_index
    }

    /// Get the length of the vector
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns whether the vector is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl<T> From<Vec<T>> for NegIdxVec<T> {
    fn from(v: Vec<T>) -> Self {
        // Need to capture the length before the borrow, and usize is a trivial copy type.
        let len = v.len();
        Self { data: v, len }
    }
}

impl<T> FromIterator<T> for NegIdxVec<T> {
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        let data = Vec::from_iter(iter);
        let len = data.len();
        Self { data, len }
    }
}

impl<T: Clone> From<&[T]> for NegIdxVec<T> {
    fn from(value: &[T]) -> Self {
        let v: Vec<T> = Vec::from(value);
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
    use pretty_assertions::assert_eq;
    use rstest::rstest;

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
    fn test_is_empty() {
        {
            let vec = NegIdxVec::<u8>::default();
            assert!(vec.is_empty());
            assert_eq!(vec.len(), 0);
        }
        {
            let vec = NegIdxVec::<u8>::from(vec![0, 1, 2, 3]);
            assert!(!vec.is_empty());
            assert_eq!(vec.len(), 4);
        }
    }

    #[test]
    #[should_panic]
    fn test_negative_overflow() {
        let vec = NegIdxVec::<u32>::from(test_vector());
        let idx = (vec.len() as i32) * -2;
        let _ = vec[idx];
    }

    #[rstest]
    #[case(1)]
    #[case(2)]
    #[case(10)]
    fn test_create_new_with_size(#[case] size: usize) {
        let vec = NegIdxVec::<u32>::new(size, Default::default);
        assert_eq!(vec.len(), size);
    }

    #[rstest]
    #[case(1)]
    #[case(10)]
    #[case(200)]
    fn test_reserve_inexact(#[case] additional_elements: usize) {
        let mut vec = NegIdxVec::<u8>::default();
        assert_eq!(vec.len(), 0);
        vec.reserve(additional_elements);
        assert!(vec.capacity() >= additional_elements);
    }

    #[test]
    fn test_create_default() {
        let vec = NegIdxVec::<u8>::default();
        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_into_iter() {
        let source_vec: Vec<i32> = vec![0, 1, 2, 3, 10, 49];
        let neg_idx_vec: NegIdxVec<i32> = NegIdxVec::from(&source_vec[..]);
        let collected_vec: Vec<i32> = neg_idx_vec.into_iter().collect();
        assert_eq!(source_vec, collected_vec);
    }

    #[test]
    fn test_from_iter() {
        let source_vec: Vec<i32> = vec![0, 1, 2, 3, 10, 49];
        let neg_idx_vec = NegIdxVec::from_iter(source_vec.clone());
        let extracted_vec: Vec<i32> = neg_idx_vec.into_iter().collect();
        assert_eq!(source_vec, extracted_vec);
    }
}
