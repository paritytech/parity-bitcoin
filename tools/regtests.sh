#!/bin/bash

echo 'Running regtests from ./tools/compare-tool/pull-tests-be0eef7.jar' && echo -en 'travis_fold:start:regtests'

./target/release/pbtc --regtest --db-cache=192 &
! java -jar ./tools/compare-tool/pull-tests-be0eef7.jar /tmp/regtest-db 2>&1 | tee regtests-full.log | grep -E --color=auto 'org.bitcoinj.store.BlockStoreException\:|BitcoindComparisonTool.main\: ERROR|bitcoind sent us a block it already had, make sure bitcoind has no blocks!|java.lang.NullPointerException'
result=$?

if [ $result -eq 1 ]
then
  echo "Regtests failed" | grep --color=auto "failed"
  echo "-----------------------------"
  echo "Full log: "
  cat regtests-full.log
else
  echo "Reg tests ok, test cases: "
  GREP_COLOR="01;32" grep -E "BitcoindComparisonTool.main: Block \"b[0-9]*\" completed processing" regtests-full.log
fi

echo -en 'travis_fold:end:regtests'

pkill -f ./target/release/pbtc
exit "$result"

