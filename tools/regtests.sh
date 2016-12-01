#!/bin/bash

./target/release/pbtc --regtest &
! java -jar ./tools/test-scripts/pull-tests-f56eec3.jar /tmp/regtest-db 2>&1 | grep -E 'org.bitcoinj.store.BlockStoreException\:|BitcoindComparisonTool.main\: ERROR'
result=$?
pkill -f ./target/release/pbtc
exit "$result"
