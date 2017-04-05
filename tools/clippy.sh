#!/bin/bash

# NOTE!
# this script requires nightly version of rustc
# clippy plugin needs to be compiled with the same nigtly version as currently set one
# tested with clippy 0.0.98

# temporary workaround for cargo clippy issue #1330
# https://github.com/Manishearth/rust-clippy/issues/1330

# first, let's enter any directory in a workspace with target kind `lib`
# https://github.com/Manishearth/rust-clippy/blob/6a73c8f8e3f513f6a16c6876be3d326633dbc78d/src/main.rs#L186
cd primitives

# now let's run clippy
# clippy does not support multiple packages, so let's run them one after another
cargo clippy -p bitcrypto
cargo clippy -p chain
cargo clippy -p db
cargo clippy -p import
cargo clippy -p keys
cargo clippy -p message
cargo clippy -p miner
cargo clippy -p network
cargo clippy -p p2p
cargo clippy -p primitives
cargo clippy -p rpc
cargo clippy -p script
cargo clippy -p serialization
cargo clippy -p sync
cargo clippy -p test-data
cargo clippy -p verification

