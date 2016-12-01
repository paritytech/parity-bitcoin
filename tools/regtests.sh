#!/bin/bash

./target/release/pbtc --regtest --db-cache=192 &
! java -jar ./tools/test-scripts/pull-tests-f56eec3.jar /tmp/regtest-db 2>&1 | grep -E --color=auto 'org.bitcoinj.store.BlockStoreException\:|BitcoindComparisonTool.main\: ERROR|bitcoind sent us a block it already had, make sure bitcoind has no blocks!|java.lang.NullPointerException'
result=$?
pkill -f ./target/release/pbtc
exit "$result"
