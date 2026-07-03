//! A dense band matrix for bounded tree edit distance computations.
//!
//! Both matrices in the paper are diagonal bands: the subtree-distance matrix
//! `TD` is banded by the postorder difference `|post(x) − post(y)| ≤ τ`
//! (Alg. 2, line 2), and each forest-distance matrix `FD` is banded by the
//! *local* index difference `≤ ε(x, y, τ)` (edits pruning, Fig. 4). Cells
//! outside the band are never stored and read back as [`INF`], which the DP
//! treats as "unreachable within the budget".

/// Effectively-infinite distance; large enough that `INF + 1` cannot wrap.
pub(crate) const INF: u32 = u32::MAX / 2;

/// A `rows × cols` matrix that only stores cells with `|col − row| ≤ h`.
#[derive(Debug, Clone)]
pub(crate) struct BandMatrix {
    rows: usize,
    cols: usize,
    half_width: usize,
    data: Vec<u32>,
}

impl BandMatrix {
    /// Create a band matrix with every stored cell initialized to [`INF`].
    ///
    /// `half_width` is clamped to `rows.max(cols)`: a wider band cannot hold
    /// any additional valid cells, and clamping keeps memory proportional to
    /// the matrix instead of the requested bound.
    pub(crate) fn new(rows: usize, cols: usize, half_width: usize) -> Self {
        let half_width = half_width.min(rows.max(cols));
        let width = 2 * half_width + 1;
        BandMatrix {
            rows,
            cols,
            half_width,
            data: vec![INF; rows * width],
        }
    }

    #[allow(dead_code)] // consumed by topdiff.rs (Task 6)
    pub(crate) fn half_width(&self) -> usize {
        self.half_width
    }

    /// Storage slot for `(r, c)`, or `None` when outside the band or matrix.
    fn index(&self, r: usize, c: usize) -> Option<usize> {
        if r >= self.rows || c >= self.cols {
            return None;
        }
        let k = c as i64 - r as i64;
        if k.abs() > self.half_width as i64 {
            return None;
        }
        let width = 2 * self.half_width + 1;
        Some(r * width + (k + self.half_width as i64) as usize)
    }

    /// Read a cell; out-of-band or out-of-range cells are [`INF`].
    pub(crate) fn get(&self, r: usize, c: usize) -> u32 {
        self.index(r, c).map_or(INF, |i| self.data[i])
    }

    /// Write a cell. Callers must stay inside the band (loop bounds come from
    /// [`Self::row_cols`]), which `debug_assert` verifies.
    pub(crate) fn set(&mut self, r: usize, c: usize, v: u32) {
        let idx = self.index(r, c);
        debug_assert!(idx.is_some(), "BandMatrix::set out of band: ({r}, {c})");
        if let Some(i) = idx {
            self.data[i] = v;
        }
    }

    /// The in-band, in-range column indices of row `r` (may be empty).
    pub(crate) fn row_cols(&self, r: usize) -> std::ops::RangeInclusive<usize> {
        let lo = r.saturating_sub(self.half_width);
        let hi = (r + self.half_width).min(self.cols.saturating_sub(1));
        lo..=hi
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_start_at_inf() {
        let m = BandMatrix::new(4, 4, 1);
        assert_eq!(m.get(0, 0), INF);
        assert_eq!(m.get(3, 3), INF);
    }

    #[test]
    fn set_get_roundtrip_within_band() {
        let mut m = BandMatrix::new(4, 4, 1);
        m.set(0, 0, 7);
        m.set(1, 2, 9);
        m.set(2, 1, 3);
        assert_eq!(m.get(0, 0), 7);
        assert_eq!(m.get(1, 2), 9);
        assert_eq!(m.get(2, 1), 3);
    }

    #[test]
    fn out_of_band_reads_are_inf() {
        let mut m = BandMatrix::new(5, 5, 1);
        m.set(2, 2, 1);
        assert_eq!(m.get(0, 4), INF);
        assert_eq!(m.get(4, 0), INF);
        // Out of range entirely.
        assert_eq!(m.get(9, 0), INF);
        assert_eq!(m.get(0, 9), INF);
    }

    #[test]
    fn row_cols_clips_to_band_and_range() {
        let m = BandMatrix::new(5, 5, 1);
        assert_eq!(m.row_cols(0), 0..=1);
        assert_eq!(m.row_cols(2), 1..=3);
        assert_eq!(m.row_cols(4), 3..=4);
    }

    #[test]
    fn row_cols_can_be_empty() {
        // 1 row, many cols, tiny band: row 0 still sees cols 0..=h.
        let m = BandMatrix::new(1, 10, 2);
        assert_eq!(m.row_cols(0), 0..=2);
        // Row beyond all cols yields an empty range.
        let m = BandMatrix::new(10, 1, 2);
        assert!(m.row_cols(9).is_empty());
    }

    #[test]
    fn zero_width_band_is_the_diagonal() {
        let mut m = BandMatrix::new(3, 3, 0);
        m.set(1, 1, 5);
        assert_eq!(m.get(1, 1), 5);
        assert_eq!(m.get(1, 0), INF);
        assert_eq!(m.get(1, 2), INF);
        assert_eq!(m.row_cols(1), 1..=1);
    }

    #[test]
    fn half_width_clamps_to_dimensions() {
        // Storage must not scale with an absurd tau on a tiny matrix.
        let m = BandMatrix::new(3, 3, 1_000_000);
        assert_eq!(m.half_width(), 3);
        assert_eq!(m.row_cols(0), 0..=2);
    }
}
