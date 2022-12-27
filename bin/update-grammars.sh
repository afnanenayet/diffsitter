#!/usr/bin/env sh

# Update all submodules to latest HEAD

git submodule foreach "git pull"
