use anyhow::Result;
use cc;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// The top level directory for each language that contains the tree sitter source for each
/// language
static GRAMMARS_DIR: &'static str = "grammars";

/// Candidate source file names that might be in a tree sitter directory
static SRC_FILE_CANDS: &'static [&'static str] = &["parser", "scanner"];

/// All of the valid extensions
static ALL_EXTS: &'static [&'static str] = &["c", "cc", "cpp"];

/// Generated the code fo the map between the language identifiers and the function to initialize
/// the language parser
fn codegen_language_map(languages: &[String]) -> String {
    // Build a vector of the languages for code gen
    let mut map_decl =
        "\nstatic LANGUAGES: phf::Map<&'static str, unsafe extern \"C\" fn() -> Language> = phf_map! {\n".to_owned();

    for language in languages {
        map_decl += &format!("\"{}\" => tree_sitter_{},\n", language, language);
    }
    map_decl += "};\n";
    map_decl
}

/// Compile a language's grammar
fn compile_grammar(
    include: &Path,
    files: &[PathBuf],
    output_name: &str,
    compile_cpp: bool,
) -> Result<(), cc::Error> {
    if compile_cpp {
        cc::Build::new()
            .cpp(true)
            .include(include)
            .files(files)
            .warnings(false)
            .try_compile(&output_name)
    } else {
        cc::Build::new()
            .include(include)
            .files(files)
            .warnings(false)
            .try_compile(&output_name)
    }
}

fn main() -> Result<()> {
    // Create a tuple of (folder name, folder relative path) that we can reference the desired
    // output name for each compiled grammar and the path to the source code for that compiled unit
    //let _grammars = fs::read_dir(GRAMMARS_DIR)?
    //.map(|res| res.map(|e| (String::from(e.file_name().to_string_lossy()), e.path())))
    //.collect::<Result<Vec<_>, io::Error>>()?;
    // While we deal with build errors, we will temporarily just hardcode using the Rust parser,
    // since it seems to work.
    let grammars = [
        (
            "tree-sitter-rust",
            PathBuf::from("grammars/tree-sitter-rust"),
        ),
        ("tree-sitter-c", PathBuf::from("grammars/tree-sitter-c")),
    ];

    // The string represented the generated code that we get from the tree sitter grammars
    let mut codegen = String::from(
        r#"
use tree_sitter::Language;
use phf::phf_map;
"#,
    );
    let mut languages = Vec::new();
    languages.reserve(grammars.len());

    // Iterate through each grammar, find the valid source files that are in it, and add them as
    // compilation targets
    for grammar in &grammars {
        let output_name = grammar.0;
        //let output_name = &grammar.0;
        let dir = grammar.1.join("src");

        // The folder names for the grammars are hyphenated, we want to conver those to underscores
        // so we can form valid rust identifiers
        let language = output_name
            .trim_start_matches("tree-sitter-")
            .replace("-", "_");

        let c_sources: Vec<PathBuf> = SRC_FILE_CANDS
            .iter()
            .map(|base| PathBuf::from(base.to_owned()).with_extension("c".to_owned()))
            .map(|f| dir.join(f))
            .filter(|cand| cand.is_file())
            .collect();

        // Filter for source files. A source file is valid if it has file name and extension that
        // is specified by the constants above, and is a valid file
        let sources: Vec<PathBuf> = SRC_FILE_CANDS
            .iter()
            .flat_map(|&base| {
                ALL_EXTS
                    .iter()
                    .map(move |&ext| PathBuf::from(base).with_extension(ext))
            })
            .map(|f| dir.join(f))
            .filter(|cand| cand.is_file())
            .collect();

        // If there are no valid source files, don't bother trying to compile
        if sources.is_empty() {
            continue;
        }

        // FIXME
        // The heuristic for determining whether to compile the files with c or c++ has been
        // disabled until we can get builds working. For now, we just stick to C.
        /*
        // If both files have a `.c` extension, then we will compile using the C compiler,
        // otherwise the grammar supplied C++ sources.
        // let successful_compilation =
        if c_sources.len() == 2 {
            compile_grammar(&dir, &c_sources[..], &output_name, false)?;
            //false
        } else {
            compile_grammar(&dir, &sources[..], &output_name, true)?;
        }
        */
        compile_grammar(&dir, &c_sources[..], &output_name, false)?;

        // If compilation succeeded with either case, link the language
        //if successful_compilation {
        codegen += &format!(
            "extern \"C\" {{ pub fn tree_sitter_{}() -> Language; }}\n",
            language
        );
        languages.push(language);
    }
    codegen += &codegen_language_map(&languages);

    // Write the generated code to a file called `grammar.rs`
    let codegen_out_dir = env::var_os("OUT_DIR").unwrap();
    let codegen_path = Path::new(&codegen_out_dir).join("generated_grammar.rs");
    fs::write(&codegen_path, codegen).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
