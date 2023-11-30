#!/usr/bin/env sh
# Update all submodules to latest HEAD
set -ex

git submodule foreach "git pull"
git add .
git commit -m "chore(grammars): Update grammars"
