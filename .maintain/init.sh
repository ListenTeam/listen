#!/usr/bin/env bash

set -e

echo "*** Initializing WASM build environment"

if [ -z $CI_PROJECT_NAME ] ; then
   rustup update nightly-2020-07-27
   rustup update stable
fi

rustup target add wasm32-unknown-unknown --toolchain nightly-2020-07-27
