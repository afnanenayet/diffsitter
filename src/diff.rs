//! Structs and other convenience methods for handling logical concepts pertaining to diffs, such
//! as hunks.

use crate::ast::Entry;
use anyhow::Result;
use std::collections::VecDeque;
use thiserror::Error;

/// The edit information representing a line
#[derive(Debug, Clone, PartialEq)]
pub struct Line<'a> {
    /// The index of the line in the original document
    pub line_index: usize,
    /// The entries corresponding to the line
    pub entries: VecDeque<Entry<'a>>,
}

impl<'a> Line<'a> {
    pub fn new(line_index: usize) -> Self {
        Line {
            line_index,
            entries: VecDeque::new(),
        }
    }
}

/// A grouping of consecutive edit lines for a document
///
/// Every line in a hunk must be consecutive and in ascending order.
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk<'a>(pub VecDeque<Line<'a>>);

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
    #[error("Attempted to prepend an entry with a line index ({incoming_line:?}) greater than the first line's index ({first_line:?})")]
    LaterLine {
        incoming_line: usize,
        first_line: usize,
    },
    #[error("Attempted to prepend an entry with a column ({incoming_col:?}) greater than the first entry's column ({first_col:?})")]
    LaterColumn {
        incoming_col: usize,
        first_col: usize,
    },
}

impl<'a> Hunk<'a> {
    /// Create a new, empty hunk
    pub fn new() -> Self {
        Hunk(VecDeque::new())
    }

    /// Returns the first line number of the hunk
    ///
    /// This will return [None] if the internal vector is empty
    pub fn first_line(&self) -> Option<usize> {
        self.0.front().map(|x| x.line_index)
    }

    /// Returns the last line number of the hunk
    ///
    /// This will return [None] if the internal vector is empty
    pub fn last_line(&self) -> Option<usize> {
        self.0.back().map(|x| x.line_index)
    }

    /// Prepend an [entry](Entry) to a hunk
    ///
    /// Entries can only be prepended in descending order (from last to first)
    pub fn push_front(&mut self, entry: Entry<'a>) -> Result<(), HunkInsertionError> {
        let incoming_line_idx = entry.reference.start_position().row;

        // Add a new line vector if the entry has a greater line index, or if the vector is empty.
        // We ensure that the last line has the same line index as the incoming entry.
        if let Some(first_line) = self.0.front() {
            let first_line_idx = first_line.line_index;

            if incoming_line_idx > first_line_idx {
                return Err(HunkInsertionError::LaterLine {
                    incoming_line: incoming_line_idx,
                    first_line: first_line_idx,
                });
            }

            if first_line_idx - incoming_line_idx > 1 {
                return Err(HunkInsertionError::NonAdjacentHunk {
                    incoming_line: incoming_line_idx,
                    last_line: first_line_idx,
                });
            }

            // Only add a new line here if the the incoming line index is one after the last entry
            // If this isn't the case, the incoming line index must be the same as the last line
            // index, so we don't have to add a new line.
            if first_line_idx - incoming_line_idx == 1 {
                self.0.push_front(Line::new(incoming_line_idx));
            }
        } else {
            // line is empty
            self.0.push_front(Line::new(incoming_line_idx));
        }

        // Add the entry to the last line
        let first_line = self.0.front_mut().unwrap();

        // Entries must be added in order, so ensure the last entry in the line has an ending
        // column less than the incoming entry's starting column.
        if let Some(&first_entry) = first_line.entries.back() {
            let first_col = first_entry.reference.end_position().column;
            let incoming_col = entry.reference.end_position().column;

            if incoming_col > first_col {
                return Err(HunkInsertionError::LaterColumn {
                    incoming_col,
                    first_col,
                });
            }
        }
        first_line.entries.push_front(entry);
        Ok(())
    }
}

/// The hunks that correspond to a document
///
/// This type implements a helper builder function that can take
#[derive(Debug, Clone, PartialEq)]
pub struct Hunks<'a>(pub VecDeque<Hunk<'a>>);

impl<'a> Hunks<'a> {
    pub fn new() -> Self {
        Hunks(VecDeque::new())
    }

    /// Push an entry to the front of the hunks
    ///
    /// This will expand the list of hunks if necessary, though the entry must precede the foremost
    /// hunk in the document (by row/column). Failing to do so will result in an error.
    pub fn push_front(&mut self, entry: Entry<'a>) -> Result<()> {
        // If the hunk isn't empty, attempt to prepend an entry into the first hunk
        if let Some(hunk) = self.0.front_mut() {
            let res = hunk.push_front(entry);

            // If the hunk insertion fails because an entry isn't adjacent, then we can create a
            // new hunk. Otherwise we propagate the error since it is a logic error.
            if let Err(HunkInsertionError::NonAdjacentHunk {
                incoming_line: _,
                last_line: _,
            }) = res
            {
                self.0.push_front(Hunk::new());
                self.0.front_mut().unwrap().push_front(entry)?;
            } else {
                res.map_err(|x| anyhow::anyhow!(x))?;
            }
        } else {
            self.0.push_front(Hunk::new());
            self.0.front_mut().unwrap().push_front(entry)?;
        }
        Ok(())
    }
}
