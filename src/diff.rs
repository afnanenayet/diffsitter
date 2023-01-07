//! Structs and other convenience methods for handling logical concepts pertaining to diffs, such
//! as hunks.

use crate::input_processing::{EditType, Entry};
use crate::neg_idx_vec::NegIdxVec;
use anyhow::Result;
use logging_timer::time;
use serde::Serialize;
use std::fmt::Debug;
use std::iter::FromIterator;
use std::ops::Range;
use thiserror::Error;

/// Find the length of the common prefix between the ranges specified for `a` and `b`.
fn common_prefix_len<T: PartialEq>(
    a: &[T],
    a_range: Range<usize>,
    b: &[T],
    b_range: Range<usize>,
) -> usize {
    let mut l = 0;

    unsafe {
        while a_range.start + l < a_range.end
            && b_range.start + l < b_range.end
            && a.get_unchecked(a_range.start + l) == b.get_unchecked(b_range.start + l)
        {
            l += 1;
        }
    }
    l
}

/// Coordinates for different inputs
///
/// A coordinate pair that corresponds to the two inputs in a diff algorithm.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Coordinates<T>
where
    T: Debug + PartialEq + Eq + Clone + Copy,
{
    /// The index in the old input
    pub old: T,

    /// The index in the new input
    pub new: T,
}

/// Convert a range with into another numeric type.
///
/// This will panic if the values cannot be converted to the target type. This is better than using
/// `as` because it will explicitly panic instead of silently wrapping around.
fn convert_range<FromType, IntoType>(range: Range<FromType>) -> Range<IntoType>
where
    FromType: TryInto<IntoType>,
    <FromType as TryInto<IntoType>>::Error: std::fmt::Debug,
{
    range.start.try_into().unwrap()..range.end.try_into().unwrap()
}

/// Find the length of the common suffix between the ranges specified for `a` and `b`.
/// The ranges are assumed to be [inclusive, exclusive).
fn common_suffix_len<T: PartialEq>(
    a: &[T],
    a_range: Range<usize>,
    b: &[T],
    b_range: Range<usize>,
) -> usize {
    let mut l: isize = 1;
    let a_range: Range<isize> = convert_range(a_range);
    let b_range: Range<isize> = convert_range(b_range);
    unsafe {
        while a_range.end - l >= a_range.start
            && b_range.end - l >= b_range.start
            && a.get_unchecked::<usize>((a_range.end - l).try_into().unwrap())
                == b.get_unchecked::<usize>((b_range.end - l).try_into().unwrap())
        {
            l += 1;
        }
    }
    (l - 1).try_into().unwrap()
}

/// The edit information representing a line
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line<'a> {
    /// The index of the line in the original document
    pub line_index: usize,
    /// The entries corresponding to the line
    pub entries: Vec<Entry<'a>>,
}

impl<'a> Line<'a> {
    pub fn new(line_index: usize) -> Self {
        Line {
            line_index,
            entries: Vec::new(),
        }
    }
}

/// A grouping of consecutive edit lines for a document
///
/// Every line in a hunk must be consecutive and in ascending order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk<'a>(pub Vec<Line<'a>>);

/// Types of errors that come up when inserting an entry to a hunk
#[derive(Debug, Error)]
pub enum HunkInsertionError {
    #[error(
        "Non-adjacent entry (line {incoming_line:?}) added to hunk (last line: {last_line:?})"
    )]
    NonAdjacentHunk {
        incoming_line: usize,
        last_line: usize,
    },

    #[error("Attempted to append an entry with a line index ({incoming_line:?}) less than the first line's index ({last_line:?})")]
    PriorLine {
        incoming_line: usize,
        last_line: usize,
    },

    #[error("Attempted to append an entry with a column ({incoming_col:?}, line: {incoming_line:?}) less than the first entry's column ({last_col:?}, line: {last_line:?})")]
    PriorColumn {
        incoming_col: usize,
        incoming_line: usize,
        last_col: usize,
        last_line: usize,
    },
}

impl<'a> Hunk<'a> {
    /// Create a new, empty hunk
    pub fn new() -> Self {
        Hunk(Vec::new())
    }

    /// Returns the first line number of the hunk
    ///
    /// This will return [None] if the internal vector is empty
    pub fn first_line(&self) -> Option<usize> {
        self.0.first().map(|x| x.line_index)
    }

    /// Returns the last line number of the hunk
    ///
    /// This will return [None] if the internal vector is empty
    pub fn last_line(&self) -> Option<usize> {
        self.0.last().map(|x| x.line_index)
    }

    /// Append an [entry](Entry) to a hunk.
    ///
    /// Entries can only be appended in ascending order (first to last). It is an error to append
    /// entries out of order. For example, you can't insert an entry on line 1 after inserting an
    /// entry on line 5.
    pub fn push_back(&mut self, entry: Entry<'a>) -> Result<(), HunkInsertionError> {
        let incoming_line_idx = entry.start_position().row;

        // Create a new line if the incoming entry is on the next line. This will throw an error
        // if we have an entry on a non-adjacent line or an out-of-order insertion.
        if let Some(last_line) = self.0.last() {
            let last_line_idx = last_line.line_index;

            if incoming_line_idx < last_line_idx {
                return Err(HunkInsertionError::PriorLine {
                    incoming_line: incoming_line_idx,
                    last_line: last_line_idx,
                });
            }

            if incoming_line_idx - last_line_idx > 1 {
                return Err(HunkInsertionError::NonAdjacentHunk {
                    incoming_line: incoming_line_idx,
                    last_line: last_line_idx,
                });
            }

            if incoming_line_idx - last_line_idx == 1 {
                self.0.push(Line::new(incoming_line_idx));
            }
        }
        // The lines are empty, we need to add the first one
        else {
            self.0.push(Line::new(incoming_line_idx));
        }

        let last_line = self.0.last_mut().unwrap();

        if let Some(&last_entry) = last_line.entries.last() {
            let last_col = last_entry.end_position().column;
            let last_line = last_entry.end_position().row;
            let incoming_col = entry.start_position().column;
            let incoming_line = entry.end_position().row;

            if incoming_col < last_col {
                return Err(HunkInsertionError::PriorColumn {
                    incoming_col,
                    last_col,
                    incoming_line,
                    last_line,
                });
            }
        }
        last_line.entries.push(entry);
        Ok(())
    }
}

/// A generic type for diffs that source from one of two documents.
///
/// A lot of items in the diff are delineated by whether they come from the old document or the new
/// one. This enum generically defines an enum wrapper over those document types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DocumentType<T: Debug + Clone + PartialEq + Serialize> {
    Old(T),
    New(T),
}

impl<T> AsRef<T> for DocumentType<T>
where
    T: Debug + Clone + PartialEq + Serialize,
{
    fn as_ref(&self) -> &T {
        match self {
            Self::Old(x) | Self::New(x) => x,
        }
    }
}

impl<T> AsMut<T> for DocumentType<T>
where
    T: Debug + Clone + PartialEq + Serialize,
{
    fn as_mut(&mut self) -> &mut T {
        match self {
            Self::Old(x) | Self::New(x) => x,
        }
    }
}

impl<T: Debug + Clone + PartialEq + Serialize> DocumentType<T> {
    /// Move the inner object out and consume it.
    fn consume(self) -> T {
        match self {
            Self::Old(x) | Self::New(x) => x,
        }
    }
}

/// A hunk with metadata about which document it came from.
pub type RichHunk<'a> = DocumentType<Hunk<'a>>;

/// The hunks that correspond to a document
///
/// This type implements a helper builder function that can take
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunks<'a>(pub Vec<Hunk<'a>>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RichHunks<'a>(pub Vec<RichHunk<'a>>);

/// A builder struct for [`RichHunks`].
///
/// The builder struct allows us to maintain some state as we build [`RichHunks`].
pub struct RichHunksBuilder<'a> {
    /// The hunks that we're trying to build
    hunks: RichHunks<'a>,

    /// The last old entry seen so far (if any).
    last_old: Option<usize>,

    /// The last new entry seen so far (if any).
    last_new: Option<usize>,
}

impl<'a> RichHunksBuilder<'a> {
    pub fn new() -> Self {
        Self {
            hunks: RichHunks(Vec::new()),
            last_old: None,
            last_new: None,
        }
    }

    /// Finalize building the hunks.
    ///
    /// This consumes the builder, which means you won't be able to add to it any more.
    pub fn build(self) -> RichHunks<'a> {
        self.hunks
    }

    /// Initialize a new hunk struct for the incoming entry if necessary.
    ///
    /// This returns the hunk that the entry should be added to.
    fn get_hunk_for_insertion(
        &mut self,
        incoming_entry: &DocumentType<Entry<'a>>,
    ) -> Result<usize, HunkInsertionError> {
        let (mut last_idx, new_hunk) = match incoming_entry {
            DocumentType::Old(_) => (self.last_old, DocumentType::Old(Hunk::new())),
            DocumentType::New(_) => (self.last_new, DocumentType::New(Hunk::new())),
        };

        match last_idx {
            // If there is no hunk for this type, we create a new one and will add the incoming
            // entry to it.
            None => {
                self.hunks.0.push(new_hunk);
                last_idx = Some(self.hunks.0.len() - 1);
            }
            // If there is a reference to the last hunk that corresponds to the incoming entry's
            // document type, we check the line numbers and create a new hunk if necessary.
            Some(idx) => {
                let last_hunk = self.hunks.0[idx].as_ref();
                let last_line = last_hunk.last_line();

                // If the hunk is populated, we only add to it if the incoming entry is on the same
                // line as the last hunk for the same type. Otherwise we break and create a new
                // one. If the hunk is empty, we can obviously add to it, so we do nothing.
                if let Some(last_line) = last_line {
                    let incoming_line = incoming_entry.as_ref().end_position.row;

                    if incoming_line < last_line {
                        return Err(HunkInsertionError::PriorLine {
                            incoming_line,
                            last_line,
                        });
                    }

                    // If we have a non-adjacent edit, we need to create a new hunk. If the last
                    // hunk for this document type isn't the last edit, we need to create a new
                    // hunk because the edit chain has been broken.
                    if incoming_line - last_line > 1 {
                        self.hunks.0.push(new_hunk);
                        last_idx = Some(self.hunks.0.len() - 1);
                    }

                    // Otherwise, the line number must be equal
                }
            }
        };

        match incoming_entry {
            DocumentType::Old(_) => self.last_old = last_idx,
            DocumentType::New(_) => self.last_new = last_idx,
        }
        Ok(last_idx.unwrap())
    }

    /// Add an entry to the hunks.
    pub fn push_back(&mut self, entry_wrapper: DocumentType<Entry<'a>>) -> Result<()> {
        let insertion_idx = self.get_hunk_for_insertion(&entry_wrapper)?;
        self.hunks.0[insertion_idx]
            .as_mut()
            .push_back(entry_wrapper.consume())?;
        Ok(())
    }
}

impl<'a> Hunks<'a> {
    pub fn new() -> Self {
        Hunks(Vec::new())
    }

    pub fn push_back(&mut self, entry: Entry<'a>) -> Result<()> {
        if let Some(hunk) = self.0.last_mut() {
            let res = hunk.push_back(entry);

            // If the incoming edit is not adjacent that means we need to create a new edit hunk
            if let Err(HunkInsertionError::NonAdjacentHunk {
                incoming_line: _,
                last_line: _,
            }) = res
            {
                self.0.push(Hunk::new());
                self.0.last_mut().unwrap().push_back(entry)?;
            } else {
                res.map_err(|x| anyhow::anyhow!(x))?;
            }
        } else {
            self.0.push(Hunk::new());
            self.0.last_mut().unwrap().push_back(entry)?;
        }
        Ok(())
    }
}

pub struct HunkAppender<'a>(pub Hunks<'a>);

impl<'a> FromIterator<Entry<'a>> for HunkAppender<'a> {
    /// Create an instance of `Hunks` from an iterator over [entries](Entry).
    ///
    /// The user is responsible for making sure that the hunks are in proper order, otherwise this
    /// constructor may panic.
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Entry<'a>>,
    {
        let mut hunks = Hunks::new();

        for i in iter {
            hunks.push_back(i).expect("Invalid iterator");
        }
        HunkAppender(hunks)
    }
}

/// A difference engine provider
///
/// Any entity that implements this trait is responsible for providing a method
/// that computes the diff between two inputs.
pub trait Engine<'elem, T>
where
    T: Eq + 'elem,
{
    /// The container type to returned from the `diff` function
    type Container;

    /// Compute the shortest edit sequence that will turn `a` into `b`
    fn diff(&self, a: &'elem [T], b: &'elem [T]) -> Self::Container;
}

#[derive(Eq, PartialEq, Copy, Clone, Debug, Default)]
pub struct Myers {}

impl<'elem, T> Engine<'elem, T> for Myers
where
    T: Eq + 'elem + std::fmt::Debug,
{
    type Container = Vec<EditType<&'elem T>>;

    fn diff(&self, a: &'elem [T], b: &'elem [T]) -> Self::Container {
        let mut res = Vec::new();
        // We know the worst case is deleting everything from a and inserting everything from b
        res.reserve(a.len() + b.len());
        let mut frontiers = MyersFrontiers::new(a.len(), b.len());
        Myers::diff_impl(&mut res, a, 0..a.len(), b, 0..b.len(), &mut frontiers);
        res
    }
}

/// Information relevant for a middle snake calculation
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MidSnakeInfo {
    /// The index in `a` that corresponds to the middle snake
    pub a_split: i32,

    /// The index in `b` that corresponds to the middle snake
    pub b_split: i32,

    /// the full length of the optimal path between the two inputs
    pub optimal_len: u32,
}

/// Split a range at the specified index
fn split_range(r: &Range<usize>, idx: usize) -> (Range<usize>, Range<usize>) {
    (r.start..idx, idx..r.end)
}

/// The frontiers for the Myers diff algorithm
///
/// We define this externally to the recursive method so we can allocate once and reuse between
/// recursive calls.
pub struct MyersFrontiers {
    /// Stores the longest path seen from the old input to the new input
    pub forward: NegIdxVec<i32>,

    /// Stores the longest path seen from the new input to the old input
    pub reverse: NegIdxVec<i32>,
}

impl MyersFrontiers {
    /// Construct frontiers for the given input sizes
    fn new(old_len: usize, new_len: usize) -> Self {
        let midpoint = ((old_len + new_len) as f32 / 2.00).ceil() as i32 + 1;

        // The size of the frontier vector
        let vec_length = (midpoint * 2) as usize;

        MyersFrontiers {
            forward: vec![0; vec_length].into(),
            reverse: vec![0; vec_length].into(),
        }
    }
}

impl Myers {
    /// A helper implementation function that handles the recursive end of finding a diff using
    /// Myers' algorithm.
    fn diff_impl<'elem, T: Eq + Debug + 'elem>(
        res: &mut Vec<EditType<&'elem T>>,
        old: &'elem [T],
        mut old_range: Range<usize>,
        new: &'elem [T],
        mut new_range: Range<usize>,
        frontiers: &mut MyersFrontiers,
    ) {
        // Initial optimizations: we can skip the common prefix + suffix
        let common_pref_len = common_prefix_len(old, old_range.clone(), new, new_range.clone());
        old_range.start += common_pref_len;
        new_range.start += common_pref_len;

        let common_suf_len = common_suffix_len(old, old_range.clone(), new, new_range.clone());
        // Need to make sure our begin/end ranges don't overlap
        old_range.end = old_range.start.max(old_range.end - common_suf_len);
        new_range.end = new_range.start.max(new_range.end - common_suf_len);

        // We know if either or both of the inputs are empty, we don't have to bother finding the
        // middle snake
        if old_range.is_empty() && new_range.is_empty() {
            return;
        }

        if old_range.is_empty() {
            for i in new_range {
                res.push(EditType::Addition(&new[i]));
            }
            return;
        }

        if new_range.is_empty() {
            for i in old_range {
                res.push(EditType::Deletion(&old[i]));
            }
            return;
        }

        let Coordinates {
            old: x_start,
            new: y_start,
        } = Myers::middle_snake(old, old_range.clone(), new, new_range.clone(), frontiers);

        // divide and conquer along the middle snake
        let (old_first_half, old_second_half) = split_range(&old_range, x_start);
        let (new_first_half, new_second_half) = split_range(&new_range, y_start);

        Myers::diff_impl(res, old, old_first_half, new, new_first_half, frontiers);
        Myers::diff_impl(res, old, old_second_half, new, new_second_half, frontiers);
    }

    /// Calculate the (x, y) coordinates of the midpoint of the optimal path.
    ///
    /// This implementation directly derives from "An O(ND) Difference Algorithm and Its Variations"
    /// by Myers. This will compute the location of the middle snake and the length of the optimal
    /// shortest edit script.
    fn middle_snake<T: Eq>(
        old: &[T],
        old_range: Range<usize>,
        new: &[T],
        new_range: Range<usize>,
        frontiers: &mut MyersFrontiers,
    ) -> Coordinates<usize> {
        let n = old_range.len() as i32;
        let m = new_range.len() as i32;
        let delta = n - m;
        let is_odd = delta % 2 == 1;
        let midpoint = ((m + n) as f32 / 2.00).ceil() as i32 + 1;

        let fwd_front = &mut frontiers.forward;
        let rev_front = &mut frontiers.reverse;

        fwd_front[1] = 0;
        rev_front[1] = 0;

        for d in 0..=midpoint {
            // Find the end of the furthest reaching forward d-path
            for k in (-d..=d).rev().step_by(2) {
                // k == -d and k != d are just bounds checks to make sure we don't try to compare
                // values outside of the [-d, d] range. We check for the furthest reaching forward
                // frontier by seeing which diagonal has the highest x value.
                let mut x = if k == -d || (k != d && fwd_front[k + 1] >= fwd_front[k - 1]) {
                    // If the longest diagonal is from the vertically connected d - 1 path. The y
                    // component is implicitly added when we compute y below with a different k value.
                    fwd_front[k + 1]
                } else {
                    // If the longest diagonal is from the horizontally connected d - 1 path. We
                    // add one here for the horizontal connection (x, y) -> (x + 1, y).
                    fwd_front[k - 1] + 1
                };
                let y = x - k;

                // Coordinates of the first point in the snake
                let (x0, y0) = (x, y);

                // Extend the snake
                if x < n && y < m {
                    debug_assert!(x >= 0);
                    debug_assert!(y >= 0);

                    let common_pref_len = common_prefix_len(
                        old,
                        old_range.start + (x as usize)..old_range.end,
                        new,
                        new_range.start + (y as usize)..new_range.end,
                    );
                    x += common_pref_len as i32;
                }

                fwd_front[k] = x;

                // If delta is odd and k is in the defined range
                if is_odd && (k - delta).abs() < d {
                    // If the path overlaps the furthest reaching reverse d - 1 path in diagonal k
                    // then the length of an SES is 2D - 1, and the last snake of the forward path
                    // is the middle snake.
                    let reverse_x = rev_front[-(k - delta)];

                    // We convert everything over to signed integers first so we can check for any
                    // overflow errors.
                    let old = (old_range.start as i32) + x0;
                    let new = (new_range.start as i32) + y0;

                    debug_assert!(
                        old >= (old_range.start as i32) && old <= (old_range.end as i32),
                        "expected old={} in {}..{}",
                        old,
                        old_range.start,
                        old_range.end,
                    );
                    debug_assert!(
                        new >= (new_range.start as i32) && new <= (new_range.end as i32),
                        "expected new={} in {}..{}",
                        new,
                        new_range.start,
                        new_range.end,
                    );

                    // NOTE: we can convert x and y to `usize` because they are both within
                    // the range of the length of the inputs, which are valid usize values. This property
                    // is also checked with assertions in debug releases.
                    if x + reverse_x >= n {
                        return Coordinates {
                            old: old as usize,
                            new: new as usize,
                        };
                    }
                }
            }

            // Find the end of the furthest reaching reverse d-path
            for k in (-d..=d).rev().step_by(2) {
                // k == d and k != -d are just bounds checks to make sure we don't try to compare
                // anything out of range, as explained above. In the reverse path we check to see
                // which diagonal has the smallest *real* x value because we're trying to go from
                // the bottom-right to the top-left of the matrix. Note that we're looking for the
                // biggest x value in the reverse frontier, which will be subtracted from the total
                // length.
                let mut x = if k == -d || (k != d && rev_front[k + 1] >= rev_front[k - 1]) {
                    // If the longest diagonal is from the horizontally connected d - 1 path.
                    rev_front[k + 1]
                } else {
                    // If the longest diagonal is from the vertically connected d - 1 path. The y
                    // value is implicitly handled when we compute y with a different k value.
                    rev_front[k - 1] + 1
                };
                let mut y = x - k;

                // Advance the diagonal as far as possible
                if x < n && y < m {
                    debug_assert!(x >= 0);
                    debug_assert!(y >= 0);
                    debug_assert!(n - x >= 0);
                    debug_assert!(m - y >= 0);

                    let common_suf_len = common_suffix_len(
                        old,
                        old_range.start..old_range.start + (n as usize) - (x as usize),
                        new,
                        new_range.start..new_range.start + (m as usize) - (y as usize),
                    );
                    x += common_suf_len as i32;
                    y += common_suf_len as i32;
                }
                rev_front[k] = x;

                // If delta is even and k is in the defined range, check for an overlap
                if !is_odd && (k - delta).abs() <= d {
                    let forward_x = fwd_front[-(k - delta)];

                    // If forward_x + reverse_x >= n, the forward and backward paths make up a full
                    // path, so we have a possible overlap. So return the furthest reaching reverse
                    // path as the middle snake.
                    // NOTE: that we can convert x and y to `usize` because they are both within
                    // the range of the length of the inputs, which are valid usize values.
                    if forward_x + x >= n {
                        let old = n - x + (old_range.start as i32);
                        let new = m - y + (new_range.start as i32);

                        debug_assert!(
                            old >= (old_range.start as i32) && old <= (old_range.end as i32),
                            "expected old={} in {}..{}",
                            old,
                            old_range.start,
                            old_range.end,
                        );
                        debug_assert!(
                            new >= (new_range.start as i32) && new <= (new_range.end as i32),
                            "expected new={} in {}..{}",
                            new,
                            new_range.start,
                            new_range.end,
                        );

                        return Coordinates {
                            old: old as usize,
                            new: new as usize,
                        };
                    }
                }
            }
        }
        unreachable!();
    }
}

impl<'a> TryFrom<Vec<EditType<&Entry<'a>>>> for RichHunks<'a> {
    type Error = anyhow::Error;

    fn try_from(edits: Vec<EditType<&Entry<'a>>>) -> Result<Self, Self::Error> {
        let mut builder = RichHunksBuilder::new();

        for edit_wrapper in edits {
            let edit = match edit_wrapper {
                EditType::Addition(&edit) => DocumentType::New(edit),
                EditType::Deletion(&edit) => DocumentType::Old(edit),
            };
            builder.push_back(edit)?;
        }

        Ok(builder.build())
    }
}

/// Compute the hunks corresponding to the minimum edit path between two documents.
///
/// This will process the the AST vectors with the user-provided settings.
///
/// This will return two groups of [hunks](diff::Hunks) in a tuple of the form
/// `(old_hunks, new_hunks)`.
#[time("info", "diff::{}")]
pub fn compute_edit_script<'a>(old: &[Entry<'a>], new: &[Entry<'a>]) -> Result<RichHunks<'a>> {
    let myers = Myers::default();
    let edit_script = myers.diff(old, new);
    RichHunks::try_from(edit_script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq as p_assert_eq;
    use test_case::test_case;

    /// A convenience function to invoke the a Myers diff
    fn myers_diff<'a, T>(a: &'a [T], b: &'a [T]) -> Vec<EditType<&'a T>>
    where
        T: 'a + Eq + Debug,
    {
        let myers = Myers::default();
        myers.diff(a, b)
    }

    #[test]
    fn mid_snake_empty_input() {
        let input_a = b"";
        let input_b = b"";

        let mut frontiers = MyersFrontiers::new(input_a.len(), input_b.len());
        let mid_snake = Myers::middle_snake(
            &input_a[..],
            0..input_a.len(),
            &input_b[..],
            0..input_b.len(),
            &mut frontiers,
        );
        let expected = Coordinates { old: 0, new: 0 };
        p_assert_eq!(expected, mid_snake);
    }

    #[test]
    fn mid_snake() {
        let input_a = &b"ABCABBA"[..];
        let input_b = &b"CBABAC"[..];
        let mut frontiers = MyersFrontiers::new(input_a.len(), input_b.len());
        let mid_snake = Myers::middle_snake(
            input_a,
            0..input_a.len(),
            input_b,
            0..input_b.len(),
            &mut frontiers,
        );
        let expected = Coordinates { old: 4, new: 1 };
        p_assert_eq!(expected, mid_snake);
    }

    #[test]
    fn myers_diff_empty_inputs() {
        let input_a: Vec<i32> = vec![];
        let input_b: Vec<i32> = vec![];
        let edit_script = myers_diff(&input_a, &input_b);
        assert!(edit_script.is_empty());
    }

    #[test]
    fn myers_diff_no_diff() {
        let input_a: Vec<i32> = vec![0; 4];
        let input_b: Vec<i32> = vec![0; 4];
        let edit_script = myers_diff(&input_a, &input_b);
        assert!(edit_script.is_empty());
    }

    #[test]
    fn myers_diff_one_addition() {
        let input_a: Vec<i32> = Vec::new();
        let input_b: Vec<i32> = vec![0];
        let expected = vec![EditType::Addition(&input_b[0])];
        let edit_script = myers_diff(&input_a, &input_b);
        p_assert_eq!(expected, edit_script);
    }

    #[test]
    fn myers_diff_one_deletion() {
        let input_a: Vec<i32> = vec![0];
        let input_b: Vec<i32> = Vec::new();
        let expected = vec![EditType::Deletion(&input_a[0])];
        let edit_script = myers_diff(&input_a, &input_b);
        p_assert_eq!(expected, edit_script);
    }

    #[test]
    fn myers_diff_single_substitution() {
        let myers = Myers::default();
        let input_a = vec![1];
        let input_b = vec![2];
        let edit_script = myers.diff(&input_a[..], &input_b[..]);
        let expected = vec![
            EditType::Addition(&input_b[0]),
            EditType::Deletion(&input_a[0]),
        ];
        p_assert_eq!(expected, edit_script);
    }

    #[test]
    fn myers_diff_single_substitution_with_common_elements() {
        let myers = Myers::default();
        let input_a = vec![0, 0, 0];
        let input_b = vec![0, 1, 0];
        let edit_script = myers.diff(&input_a[..], &input_b[..]);
        let expected = vec![
            EditType::Addition(&input_b[1]),
            EditType::Deletion(&input_a[1]),
        ];
        p_assert_eq!(expected, edit_script);
    }

    #[test_case(b"BAAA", b"CAAA" => 0 ; "no common prefix")]
    #[test_case(b"AAABA", b"AAACA" => 3 ; "with common prefix")]
    fn common_prefix(a: &[u8], b: &[u8]) -> usize {
        common_prefix_len(a, 0..a.len(), b, 0..b.len())
    }

    #[test_case(b"AAAB", b"AAAC" => 0 ; "no common suffix")]
    #[test_case(b"ABAAA", b"ACAAA" => 3 ; "with common suffix")]
    fn common_suffix(a: &[u8], b: &[u8]) -> usize {
        common_suffix_len(a, 0..a.len(), b, 0..b.len())
    }
}
