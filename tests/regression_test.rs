#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use libdiffsitter::{
        diff::{compute_edit_script, DocumentType, Hunk, RichHunks},
        generate_ast_vector_data,
        input_processing::{Entry, TreeSitterProcessor},
        parse::GrammarConfig,
    };
    use std::path::PathBuf;
    use test_case::test_case;

    fn generate_snapshot_entries_string(entries: &[&Entry<'_>]) -> String {
        let each_entry: Vec<String> = entries
            .iter()
            .map(|entry| {
                format!(
                    // Keep the indent so the snapshot diffs are more aesthetically pleasing
                    "    Entry{{'{}', start=({}, {}), end=({}, {})}}",
                    entry.text,
                    entry.start_position.row,
                    entry.start_position.column,
                    entry.end_position.row,
                    entry.end_position.column,
                )
            })
            .collect();
        each_entry.join("\n")
    }

    fn generate_snapshot_hunk_string(hunk: &Hunk) -> String {
        hunk.0
            .iter()
            .map(|line| {
                format!(
                    // Added the newlines so the diffs from `insta` are easier to parse
                    "Line={{line_index={}, entries=\n[\n{}\n]}}\n",
                    line.line_index,
                    &generate_snapshot_entries_string(line.entries.as_slice())
                )
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Generate a string representation of a rich hunk object so it can be compared using the
    /// `insta` snapshot library.
    ///
    /// We use this instead of the [Debug] representation of the type because the debug
    /// representation includes fields like kind_id that are a little more finnicky and break tests
    /// often.
    fn generate_snapshot_rich_hunks_string(hunks: RichHunks<'_>) -> String {
        hunks
            .0
            .iter()
            .map(|document_type| match document_type {
                DocumentType::Old(hunk) => format!("Old({})", generate_snapshot_hunk_string(hunk)),
                DocumentType::New(hunk) => format!("New({})", generate_snapshot_hunk_string(hunk)),
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Get paths to input files for tests
    fn get_test_paths(test_type: &str, test_name: &str, ext: &str) -> (PathBuf, PathBuf) {
        let test_data_root = PathBuf::from(format!("./test_data/{test_type}/{test_name}"));
        let path_a = test_data_root.join(format!("a.{ext}"));
        let path_b = test_data_root.join(format!("b.{ext}"));
        assert!(
            path_a.exists(),
            "test data path {} does not exist",
            path_a.to_str().unwrap()
        );
        assert!(
            path_b.exists(),
            "test data path {} does not exist",
            path_b.to_str().unwrap()
        );

        (path_a, path_b)
    }

    #[test_case("short", "rust", "rs", true, true)]
    #[test_case("short", "python", "py", true, true)]
    #[test_case("short", "go", "go", true, true)]
    #[test_case("medium", "rust", "rs", true, false)]
    #[test_case("medium", "rust", "rs", false, false)]
    #[test_case("medium", "cpp", "cpp", true, true)]
    #[test_case("medium", "cpp", "cpp", false, true)]
    #[test_case("short", "markdown", "md", true, true)]
    fn diff_hunks_snapshot(
        test_type: &str,
        name: &str,
        ext: &str,
        split_graphemes: bool,
        strip_whitespace: bool,
    ) {
        let (path_a, path_b) = get_test_paths(test_type, name, ext);
        let config = GrammarConfig::default();
        let ast_data_a = generate_ast_vector_data(path_a, None, &config).unwrap();
        let ast_data_b = generate_ast_vector_data(path_b, None, &config).unwrap();

        let processor = TreeSitterProcessor {
            split_graphemes,
            strip_whitespace,
            ..Default::default()
        };

        let diff_vec_a = processor.process(
            &ast_data_a.tree,
            &ast_data_a.text,
            &ast_data_a.resolved_language,
        );
        let diff_vec_b = processor.process(
            &ast_data_b.tree,
            &ast_data_b.text,
            &ast_data_b.resolved_language,
        );
        let diff_hunks = compute_edit_script(&diff_vec_a, &diff_vec_b).unwrap();

        // We have to set the snapshot name manually, otherwise there appear to be threading issues
        // and we end up with more snapshot files than there are tests, which cause
        // nondeterministic errors.
        let snapshot_name = format!("{test_type}_{name}_split_graphemes_{split_graphemes}_strip_whitespace_{strip_whitespace}");
        let snapshot_string = generate_snapshot_rich_hunks_string(diff_hunks);
        assert_snapshot!(snapshot_name, snapshot_string);
    }
}
