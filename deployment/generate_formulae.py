"""Generate Homebrew formulae for diffsitter and tree-sitter-mcp.

Reads version and SHA256 checksums from environment variables (set by CI)
and writes formula files to the given output directory.

Usage:
    python3 deployment/generate_formulae.py OUTPUT_DIR

Environment variables:
    VERSION                                  - Git tag (e.g. "v0.9.0" or "0.9.0")
    SHORT_VERSION                            - Semver without leading v (e.g. "0.9.0")
    diffsitter_x86_64_apple_darwin_SHA256    - SHA256 for each platform archive
    diffsitter_aarch64_apple_darwin_SHA256
    diffsitter_x86_64_unknown_linux_gnu_SHA256
    diffsitter_aarch64_unknown_linux_gnu_SHA256
    tree_sitter_mcp_x86_64_apple_darwin_SHA256
    tree_sitter_mcp_aarch64_apple_darwin_SHA256
    tree_sitter_mcp_x86_64_unknown_linux_gnu_SHA256
    tree_sitter_mcp_aarch64_unknown_linux_gnu_SHA256
"""

import os
import sys
from pathlib import Path

REPO = "https://github.com/afnanenayet/diffsitter"


def sha(name: str) -> str:
    """Get a SHA256 checksum from the environment, or 'MISSING' if absent."""
    return os.environ.get(name, "MISSING")


def release_url(tag: str, binary: str, target: str) -> str:
    return f"{REPO}/releases/download/{tag}/{binary}-{target}.tar.gz"


def generate_diffsitter(tag: str, version: str) -> str:
    return f'''\
class Diffsitter < Formula
  desc "Tree-sitter based AST difftool to get meaningful semantic diffs"
  homepage "{REPO}"
  version "{version}"
  license "MIT"

  on_macos do
    on_arm do
      url "{release_url(tag, "diffsitter", "aarch64-apple-darwin")}"
      sha256 "{sha("diffsitter_aarch64_apple_darwin_SHA256")}"
    end
    on_intel do
      url "{release_url(tag, "diffsitter", "x86_64-apple-darwin")}"
      sha256 "{sha("diffsitter_x86_64_apple_darwin_SHA256")}"
    end
  end

  on_linux do
    on_arm do
      url "{release_url(tag, "diffsitter", "aarch64-unknown-linux-gnu")}"
      sha256 "{sha("diffsitter_aarch64_unknown_linux_gnu_SHA256")}"
    end
    on_intel do
      url "{release_url(tag, "diffsitter", "x86_64-unknown-linux-gnu")}"
      sha256 "{sha("diffsitter_x86_64_unknown_linux_gnu_SHA256")}"
    end
  end

  def install
    bin.install "diffsitter"
    bin.install "git-diffsitter"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/diffsitter --version")
  end
end
'''


def generate_tree_sitter_mcp(tag: str, version: str) -> str:
    return f'''\
class TreeSitterMcp < Formula
  desc "AST-aware code navigation MCP server powered by tree-sitter"
  homepage "{REPO}"
  version "{version}"
  license "MIT"

  on_macos do
    on_arm do
      url "{release_url(tag, "tree-sitter-mcp", "aarch64-apple-darwin")}"
      sha256 "{sha("tree_sitter_mcp_aarch64_apple_darwin_SHA256")}"
    end
    on_intel do
      url "{release_url(tag, "tree-sitter-mcp", "x86_64-apple-darwin")}"
      sha256 "{sha("tree_sitter_mcp_x86_64_apple_darwin_SHA256")}"
    end
  end

  on_linux do
    on_arm do
      url "{release_url(tag, "tree-sitter-mcp", "aarch64-unknown-linux-gnu")}"
      sha256 "{sha("tree_sitter_mcp_aarch64_unknown_linux_gnu_SHA256")}"
    end
    on_intel do
      url "{release_url(tag, "tree-sitter-mcp", "x86_64-unknown-linux-gnu")}"
      sha256 "{sha("tree_sitter_mcp_x86_64_unknown_linux_gnu_SHA256")}"
    end
  end

  def install
    bin.install "tree-sitter-mcp"
  end

  def caveats
    <<~EOS
      To use with Claude Code, register the MCP server:
        claude mcp add tree-sitter-mcp -- #{{bin}}/tree-sitter-mcp
    EOS
  end

  test do
    assert_predicate bin/"tree-sitter-mcp", :executable?
  end
end
'''


def main():
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} OUTPUT_DIR", file=sys.stderr)
        sys.exit(1)

    output_dir = Path(sys.argv[1])
    output_dir.mkdir(parents=True, exist_ok=True)

    tag = os.environ.get("VERSION", "")
    version = os.environ.get("SHORT_VERSION", "")

    if not tag or not version:
        print("ERROR: VERSION and SHORT_VERSION env vars are required", file=sys.stderr)
        sys.exit(1)

    formulae = {
        "diffsitter.rb": generate_diffsitter(tag, version),
        "tree-sitter-mcp.rb": generate_tree_sitter_mcp(tag, version),
    }

    for filename, content in formulae.items():
        path = output_dir / filename
        path.write_text(content)
        print(f"Wrote {path}")


if __name__ == "__main__":
    main()
