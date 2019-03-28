#!/bin/bash

set -e # fail on any error
set -u # treat unset variables as error
rm -rf deb
#get current version
grep -m 1 version Cargo.toml | awk '{print $3}' | tr -d '"' | tr -d "\n"
