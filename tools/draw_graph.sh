#!/bin/bash

# Cargo graph does not work with cargo worspaces #33
# https://github.com/kbknapp/cargo-graph/issues/33
# so first we need to patch Cargo.toml and remove workspace
patch -R Cargo.toml tools/workspace.diff

# Now let's rebuild Cargo.lock by telling cargo to update local package
cargo update -p pbtc

# And draw dependencies graph using cargo graph
cargo graph --build-shape box --build-line-style dashed > tools/graph.dot
dot -Tpng > tools/graph.png tools/graph.dot

# Finally let's bring back old Cargo.toml file
patch Cargo.toml tools/workspace.diff
