//! Integration tests for the tree diff engine on real parsed files.
#![cfg(feature = "static-grammar-libs")]

use libdiffsitter::generate_ast_vector_data;
use libdiffsitter::input_processing::TreeSitterProcessor;
use libdiffsitter::parse::GrammarConfig;
use libdiffsitter::render::{DiffPayload, DisplayData, DocumentDiffData, RenderConfig, Renderer};
use libdiffsitter::tree_diff::{StructuralDiff, TreeDiffOptions, tree_diff};
use std::path::PathBuf;

fn diff_fixtures(name: &str, ext: &str) -> StructuralDiff {
    let root = PathBuf::from(format!("./test_data/tree_diff/{name}"));
    let a = generate_ast_vector_data(
        root.join(format!("a.{ext}")),
        None,
        &GrammarConfig::default(),
    )
    .unwrap();
    let b = generate_ast_vector_data(
        root.join(format!("b.{ext}")),
        None,
        &GrammarConfig::default(),
    )
    .unwrap();
    tree_diff(
        &TreeSitterProcessor::default(),
        &a,
        &b,
        &TreeDiffOptions::default(),
    )
    .unwrap()
}

#[test]
fn fn_rename_rust() {
    let diff = diff_fixtures("fn_rename", "rs");
    // Two identifier renames (definition + call site), nothing else.
    assert_eq!(diff.distance, 2);
    insta::assert_json_snapshot!("fn_rename_rust", diff);
}

#[test]
fn add_param_python() {
    let diff = diff_fixtures("add_param", "py");
    assert!(diff.distance > 0);
    insta::assert_json_snapshot!("add_param_python", diff);
}

#[test]
fn identical_files_have_empty_diff() {
    let root = PathBuf::from("./test_data/tree_diff/fn_rename");
    let a = generate_ast_vector_data(root.join("a.rs"), None, &GrammarConfig::default()).unwrap();
    let a2 = generate_ast_vector_data(root.join("a.rs"), None, &GrammarConfig::default()).unwrap();
    let diff = tree_diff(
        &TreeSitterProcessor::default(),
        &a,
        &a2,
        &TreeDiffOptions::default(),
    )
    .unwrap();
    assert_eq!(diff.distance, 0);
    assert!(diff.edits.is_empty());
}

#[test]
fn structural_renderer_snapshot() {
    let diff = diff_fixtures("fn_rename", "rs");
    let data = DisplayData {
        diff: DiffPayload::Structural(diff),
        old: DocumentDiffData {
            filename: "a.rs",
            text: "",
        },
        new: DocumentDiffData {
            filename: "b.rs",
            text: "",
        },
    };
    // Disable colors so the snapshot has no ANSI escapes.
    console::set_colors_enabled(false);
    let renderer = RenderConfig::default()
        .get_renderer(Some("structural".into()))
        .unwrap();
    let mut buf = Vec::new();
    renderer.render(&mut buf, &data, None).unwrap();
    insta::assert_snapshot!(
        "structural_render_fn_rename",
        String::from_utf8(buf).unwrap()
    );
}
