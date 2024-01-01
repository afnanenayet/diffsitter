#!/usr/bin/env bash

# Summary:
#
# A convenience script that runs cargo with environment variables that build
# with address sanitizer support. This requires the nightly toolchain to be
# installed on the user's system.
#
# This will invoke cargo with the required environment variables so that users
# can run tests or build a target with one of the sanitizers compiled in.
#
# Users *must* provide the `DIFFSITTER_TARGET` environment variable due to a
# bug with how Cargo handles targets with building with sanitizers.
#
# Parameters:
#
# * DIFFSITTER_TARGET (env var, required): The cargo target triple to build
#   for. This is forwarded as the --target when invoking cargo. This must be
#   provided, otherwise Cargo will fail with errors when trying to build a
#   target.
# * DIFFSITTER_SANITIZER (env var, optional): The name of the sanitizer flag
#   to build with. You can find the full list of valid parameters here:
#   https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html
#
#   This will default to 'address' if not provided by the user.
#
#   The flag is forwarded as the `-Zprofile` rustc flag for regular targets and
#   when building docs.
#
# Examples:
#
# # Using the default sanitizer which this script sets to ASAN
# DIFFSITTER_TARGET=aarch64-apple-darwin ./build_with_sanitizer.bash test
#
# # Using a different sanitizer
# DIFFSITTER_TARGET=aarch64-apple-darwin DIFFSITTER_SANITIZER=leak ./build_with_sanitizer.bash test

set -exu

# We set the default to address if not provided by the user
diffsitter_sanitizer=${DIFFSITTER_SANITIZER:-address}

# We set the malloc nano zone to 0 as a workaround for this bug:
# https://stackoverflow.com/questions/64126942/malloc-nano-zone-abandoned-due-to-inability-to-preallocate-reserved-vm-space
MallocNanoZone='0' \
  RUSTFLAGS="-Zsanitizer=$diffsitter_sanitizer" \
  RUSTDOCFLAGS="-Zsanitizer=$diffsitter_sanitizer" \
  cargo +nightly $@ --target "$DIFFSITTER_TARGET"
