//! Benchmarks comparing the tree diff engine against the Myers engine on
//! synthetic Rust sources with small and large edits.

use criterion::{Criterion, criterion_group, criterion_main};
use libdiffsitter::diff::compute_edit_script;
use libdiffsitter::input_processing::{TreeSitterProcessor, VectorData};
use libdiffsitter::parse::{GrammarConfig, generate_language};
use libdiffsitter::tree_diff::{TreeDiffOptions, tree_diff};
use std::hint::black_box;
use std::path::PathBuf;

fn parse_rust(text: &str) -> VectorData {
    let language = generate_language("rust", &GrammarConfig::default()).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(text, None).unwrap();
    VectorData {
        text: text.to_string(),
        tree,
        path: PathBuf::from("bench.rs"),
        resolved_language: "rust".into(),
    }
}

/// Generate a source file with `n_fns` small functions; functions whose index
/// is in `renamed` get a different name (a "small edit" between versions).
fn make_source(n_fns: usize, renamed: &[usize]) -> String {
    let mut out = String::new();
    for i in 0..n_fns {
        let name = if renamed.contains(&i) {
            format!("renamed_{i}")
        } else {
            format!("original_{i}")
        };
        out.push_str(&format!("fn {name}(x: i64) -> i64 {{\n    x + {i}\n}}\n\n"));
    }
    out
}

fn bench_engines(c: &mut Criterion) {
    let processor = TreeSitterProcessor::default();
    let options = TreeDiffOptions::default();

    // Small edit: 200 functions, one renamed.
    let old_small = parse_rust(&make_source(200, &[]));
    let new_small = parse_rust(&make_source(200, &[100]));
    // Larger edit: 200 functions, 25 renamed.
    let old_large = parse_rust(&make_source(200, &[]));
    let renamed: Vec<usize> = (0..200).step_by(8).collect();
    let new_large = parse_rust(&make_source(200, &renamed));

    let mut group = c.benchmark_group("tree_diff_vs_myers");
    group.bench_function("topdiff_small_edit", |b| {
        b.iter(|| black_box(tree_diff(&processor, &old_small, &new_small, &options).unwrap()));
    });
    group.bench_function("topdiff_larger_edit", |b| {
        b.iter(|| black_box(tree_diff(&processor, &old_large, &new_large, &options).unwrap()));
    });
    group.bench_function("myers_small_edit", |b| {
        b.iter(|| {
            let a = processor.process_vec_data(&old_small);
            let bv = processor.process_vec_data(&new_small);
            // The hunks borrow from the processed vectors, so they cannot
            // leave the closure; a black-boxed count keeps the computation
            // observable. The topdiff arms black-box the whole owned result
            // instead — the asymmetry is forced by these lifetimes and does
            // not change what either arm computes.
            black_box(compute_edit_script(&a, &bv).unwrap().0.len())
        });
    });
    group.bench_function("myers_larger_edit", |b| {
        b.iter(|| {
            let a = processor.process_vec_data(&old_large);
            let bv = processor.process_vec_data(&new_large);
            black_box(compute_edit_script(&a, &bv).unwrap().0.len())
        });
    });
    group.finish();
}

criterion_group!(benches, bench_engines);
criterion_main!(benches);
