# diffsitter

## Summary

`diffsitter` performs diffs on text files using the AST to compute the diff
instead of using a naive text-based diff. This can give you more semantically
meaningful diff information, which will prevent diffs from getting polluted by
syntax differences, for example.

`diffstter` uses the parsers from the
[tree-sitter](https://tree-sitter.github.io/tree-sitter/) project to parse
source code. As such, the languages supported by this tool are limited by the
languages supported by the tree-sitter project. 

## Development

In order to develop for this project, you need to clone the project and
initialize all submodules (each tree-sitter grammar is added as a
subdirectory).
