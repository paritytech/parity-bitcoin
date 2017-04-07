#!/usr/bin/expect -f

# test dir path [ABSOULTE PATH ONLY!!!]
set test_dir /home/svyatonik/testing_82
# directory with pbtc sources
set pbtc_dir ~/dev/parity-bitcoin

# make test dir
system mkdir -p $test_dir
# build && copy pbtc
system cd $pbtc_dir && reset && cargo build --release && cp -f target/release/pbtc $test_dir
# copy regtests
system cp -Rf $pbtc_dir/tools/compare-tool $test_dir

# setup logging
global env
set env(RUST_LOG) sync=trace,p2p=trace,verification=trace,db=trace

# increase timeout as it takes > than 10 seconds to start
set timeout 60
while 1 {
  # cls
  system reset

  # remove pbtc db
  system rm -rf $test_dir/db
  # spawn pbtc
  spawn $test_dir/pbtc -d $test_dir/db --regtest
  set pbtc_id $spawn_id

  # remove regtests db
  system rm -f $test_dir/BitcoindComparisonTool.*
  # spawn regtests
  spawn java -jar $test_dir/compare-tool/pull-tests-be0eef7.jar $test_dir/BitcoindComparisonTool
  set regtest_id $spawn_id

  expect {
    -i $pbtc_id panic {
      puts "PANIC_MATCH: $expect_out(0,string)"
      stop_processes
      exit
    }
    -i $pbtc_id -regexp {..*} {
      exp_continue
    }
    -i $regtest_id -regexp {(.*)ERROR(.*)} {
      puts "ERR_MATCH: $expect_out(0,string)"
      stop_processes
      exit
    }
    -i $regtest_id -regexp {(.*)"b2000"(.*)} {
      puts "MATCH: $expect_out(0,string)"
      exit
    }
    -i $regtest_id -regexp {..*} {
      exp_continue
    }
  }

  set pbtc_pid [exp_pid -i $pbtc_id]
  set regtest_pid [exp_pid -i $regtest_id]
  system kill -9 $pbtc_pid
  system kill -9 $regtest_pid 
  close -i $pbtc_id
  #close -i $regtest_id
}
