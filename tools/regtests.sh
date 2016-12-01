#!/bin/bash

./target/release/pbtc --regtest --db-cache=192 &
java -jar ./tools/test-scripts/pull-tests-f56eec3.jar /tmp/regtest-db 2>&1 | tee regtests.log
echo ""
echo "-------------------------------------"
echo "Greping bugs..."
echo ""
! grep -E --color=auto 'org.bitcoinj.store.BlockStoreException\:|BitcoindComparisonTool.main\: ERROR|bitcoind sent us a block it already had, make sure bitcoind has no blocks!' regtests.log
result=$?
pkill -f ./target/release/pbtc
rm regtests.log
exit "$result"
