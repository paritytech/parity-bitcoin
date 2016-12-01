#!/bin/bash

./target/release/pbtc --regtest &
! java -jar ./tools/test-scripts/pull-tests-f56eec3.jar /tmp/regtest-db 2>&1 | grep -E 'BlockStoreException|ERROR'
result=$?
pkill -f ./target/release/pbtc
exit "$result"
