use criterion::{Criterion, black_box, criterion_group, criterion_main};
use libdiffsitter::ast_navigation::{self, NavigationDirection, ParseCache};
use libdiffsitter::parse::{self, GrammarConfig};
use std::path::{Path, PathBuf};

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

// ---------------------------------------------------------------------------
// 1. parse — benchmark parse::parse_file on short and medium Rust fixtures
// ---------------------------------------------------------------------------

fn bench_parse(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let short = fixture_path("test_data/ast_navigation/rust_sample.rs");
    let medium = fixture_path("test_data/medium/rust/a.rs");

    let mut group = c.benchmark_group("parse");

    group.bench_function("short_rust_file", |b| {
        b.iter(|| {
            parse::parse_file(black_box(&short), Some("rust"), black_box(&config))
                .expect("parse failed")
        });
    });

    group.bench_function("medium_rust_file", |b| {
        b.iter(|| {
            parse::parse_file(black_box(&medium), Some("rust"), black_box(&config))
                .expect("parse failed")
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. cache_hit — pre-populate a ParseCache, benchmark repeated get_or_parse
// ---------------------------------------------------------------------------

fn bench_cache_hit(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let short = fixture_path("test_data/ast_navigation/rust_sample.rs");
    let medium = fixture_path("test_data/medium/rust/a.rs");

    let mut group = c.benchmark_group("cache_hit");

    group.bench_function("short_rust_file", |b| {
        b.iter_batched(
            || {
                let mut cache = ParseCache::new(config.clone());
                cache
                    .get_or_parse(&short, Some("rust"))
                    .expect("initial parse failed");
                cache
            },
            |mut cache| {
                cache
                    .get_or_parse(black_box(&short), Some("rust"))
                    .expect("cache hit failed");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("medium_rust_file", |b| {
        b.iter_batched(
            || {
                let mut cache = ParseCache::new(config.clone());
                cache
                    .get_or_parse(&medium, Some("rust"))
                    .expect("initial parse failed");
                cache
            },
            |mut cache| {
                cache
                    .get_or_parse(black_box(&medium), Some("rust"))
                    .expect("cache hit failed");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. cache_miss — clear cache each iteration, benchmark the full parse path
// ---------------------------------------------------------------------------

fn bench_cache_miss(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let short = fixture_path("test_data/ast_navigation/rust_sample.rs");
    let medium = fixture_path("test_data/medium/rust/a.rs");

    let mut group = c.benchmark_group("cache_miss");

    group.bench_function("short_rust_file", |b| {
        b.iter_batched(
            || ParseCache::new(config.clone()),
            |mut cache| {
                cache
                    .get_or_parse(black_box(&short), Some("rust"))
                    .expect("parse failed");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("medium_rust_file", |b| {
        b.iter_batched(
            || ParseCache::new(config.clone()),
            |mut cache| {
                cache
                    .get_or_parse(black_box(&medium), Some("rust"))
                    .expect("parse failed");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. list_symbols — parse once, benchmark list_symbols repeatedly
// ---------------------------------------------------------------------------

fn bench_list_symbols(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let short = fixture_path("test_data/ast_navigation/rust_sample.rs");
    let medium = fixture_path("test_data/medium/rust/a.rs");

    let mut cache = ParseCache::new(config);
    let short_parsed = cache
        .get_or_parse(&short, Some("rust"))
        .expect("parse failed");
    let short_tree = short_parsed.tree.clone();
    let short_text = short_parsed.text.clone();
    let short_lang = short_parsed.language.clone();

    let medium_parsed = cache
        .get_or_parse(&medium, Some("rust"))
        .expect("parse failed");
    let medium_tree = medium_parsed.tree.clone();
    let medium_text = medium_parsed.text.clone();
    let medium_lang = medium_parsed.language.clone();

    let mut group = c.benchmark_group("list_symbols");

    group.bench_function("short_rust_file", |b| {
        b.iter(|| {
            ast_navigation::list_symbols(
                black_box(&short_tree),
                black_box(&short_text),
                black_box(&short_lang),
                "rust",
            )
        });
    });

    group.bench_function("medium_rust_file", |b| {
        b.iter(|| {
            ast_navigation::list_symbols(
                black_box(&medium_tree),
                black_box(&medium_text),
                black_box(&medium_lang),
                "rust",
            )
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. get_node_at_position — parse once, benchmark at various positions
// ---------------------------------------------------------------------------

fn bench_get_node_at_position(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let path = fixture_path("test_data/ast_navigation/rust_sample.rs");

    let mut cache = ParseCache::new(config);
    let parsed = cache
        .get_or_parse(&path, Some("rust"))
        .expect("parse failed");
    let tree = parsed.tree.clone();
    let text = parsed.text.clone();

    let positions: &[(usize, usize, &str)] = &[
        (0, 0, "file_start"),
        (5, 4, "struct_field"),   // inside struct Point { x: f64 }
        (16, 8, "function_body"), // inside Point::new
        (43, 8, "main_body"),     // inside main()
    ];

    let mut group = c.benchmark_group("get_node_at_position");

    for &(line, col, label) in positions {
        group.bench_with_input(
            criterion::BenchmarkId::new("position", label),
            &(line, col),
            |b, &(line, col)| {
                b.iter(|| {
                    ast_navigation::get_node_at_position(
                        black_box(&tree),
                        black_box(&text),
                        black_box(line),
                        black_box(col),
                    )
                    .expect("node lookup failed");
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 6. navigate — parse once, benchmark each direction from a fixed position
// ---------------------------------------------------------------------------

fn bench_navigate(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let path = fixture_path("test_data/ast_navigation/rust_sample.rs");

    let mut cache = ParseCache::new(config);
    let parsed = cache
        .get_or_parse(&path, Some("rust"))
        .expect("parse failed");
    let tree = parsed.tree.clone();
    let text = parsed.text.clone();

    // Position inside Point::new body (line 16, col 8) — has parent, children, and siblings
    let line = 16;
    let col = 8;

    let directions = [
        ("parent", NavigationDirection::Parent),
        ("first_child", NavigationDirection::FirstChild),
        ("next_sibling", NavigationDirection::NextSibling),
        ("prev_sibling", NavigationDirection::PrevSibling),
        ("next_named_sibling", NavigationDirection::NextNamedSibling),
        ("prev_named_sibling", NavigationDirection::PrevNamedSibling),
    ];

    let mut group = c.benchmark_group("navigate");

    for (label, direction) in &directions {
        group.bench_function(*label, |b| {
            b.iter(|| {
                // Some directions may fail (e.g., no prev sibling from first child).
                // We still want to measure the traversal cost including the error path.
                let _ = ast_navigation::navigate(
                    black_box(&tree),
                    black_box(&text),
                    black_box(line),
                    black_box(col),
                    direction.clone(),
                );
            });
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 7. get_scope — parse once, benchmark get_scope inside a function
// ---------------------------------------------------------------------------

fn bench_get_scope(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let path = fixture_path("test_data/ast_navigation/rust_sample.rs");

    let mut cache = ParseCache::new(config);
    let parsed = cache
        .get_or_parse(&path, Some("rust"))
        .expect("parse failed");
    let tree = parsed.tree.clone();
    let text = parsed.text.clone();

    let mut group = c.benchmark_group("get_scope");

    // Inside Point::new (line 16, col 8) — nested inside impl block
    group.bench_function("inside_function", |b| {
        b.iter(|| {
            ast_navigation::get_scope(
                black_box(&tree),
                black_box(&text),
                "rust",
                black_box(16),
                black_box(8),
            )
            .expect("scope lookup failed");
        });
    });

    // Inside main (line 43, col 4) — top-level function
    group.bench_function("inside_main", |b| {
        b.iter(|| {
            ast_navigation::get_scope(
                black_box(&tree),
                black_box(&text),
                "rust",
                black_box(43),
                black_box(4),
            )
            .expect("scope lookup failed");
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 8. run_query — parse once, benchmark running the Rust symbol query
// ---------------------------------------------------------------------------

fn bench_run_query(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let path = fixture_path("test_data/ast_navigation/rust_sample.rs");

    let mut cache = ParseCache::new(config);
    let parsed = cache
        .get_or_parse(&path, Some("rust"))
        .expect("parse failed");
    let tree = parsed.tree.clone();
    let text = parsed.text.clone();
    let language = parsed.language.clone();

    let query_str =
        ast_navigation::symbol_query_for_language("rust").expect("no rust symbol query");

    let mut group = c.benchmark_group("run_query");

    group.bench_function("rust_symbol_query", |b| {
        b.iter(|| {
            ast_navigation::run_query(
                black_box(&tree),
                black_box(&text),
                black_box(&language),
                black_box(query_str),
            )
            .expect("query failed");
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 9. get_definition — parse once, benchmark looking up a known symbol
// ---------------------------------------------------------------------------

fn bench_get_definition(c: &mut Criterion) {
    let config = GrammarConfig::default();
    let path = fixture_path("test_data/ast_navigation/rust_sample.rs");

    let mut cache = ParseCache::new(config);
    let parsed = cache
        .get_or_parse(&path, Some("rust"))
        .expect("parse failed");
    let tree = parsed.tree.clone();
    let text = parsed.text.clone();
    let language = parsed.language.clone();

    let mut group = c.benchmark_group("get_definition");

    // Look up the "Point" struct
    group.bench_function("known_symbol_Point", |b| {
        b.iter(|| {
            ast_navigation::get_definition(
                black_box(&tree),
                black_box(&text),
                black_box(&language),
                "rust",
                black_box("Point"),
            )
            .expect("definition lookup failed");
        });
    });

    // Look up the "main" function
    group.bench_function("known_symbol_main", |b| {
        b.iter(|| {
            ast_navigation::get_definition(
                black_box(&tree),
                black_box(&text),
                black_box(&language),
                "rust",
                black_box("main"),
            )
            .expect("definition lookup failed");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_cache_hit,
    bench_cache_miss,
    bench_list_symbols,
    bench_get_node_at_position,
    bench_navigate,
    bench_get_scope,
    bench_run_query,
    bench_get_definition,
);
criterion_main!(benches);
