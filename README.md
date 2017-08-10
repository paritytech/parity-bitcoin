# parity-bitcoin
The Parity Bitcoin client

[![Build Status][travis-image]][travis-url] 

Gitter [![Gitter https://gitter.im/paritytech/parity-bitcoin](https://badges.gitter.im/paritytech/parity-bitcoin.svg)](https://gitter.im/paritytech/parity-bitcoin)

- [Installing from source](#installing-from-source)

- [Installing the snap](#installing-the-snap)

- [Running tests](#running-tests)

- [Going online](#going-online)

- [Importing bitcoind database](#importing-bitcoind-database)

- [Command line interface](#command-line-interface)

- [JSON-RPC](#json-rpc)

- [Logging](#logging)

- [Internal Documentation](#internal-documentation)

- [Project Graph][graph]

[graph]: ./tools/graph.svg
[travis-image]: https://travis-ci.com/paritytech/parity-bitcoin.svg?token=DMFvZu71iaTbUYx9UypX&branch=master
[travis-url]: https://travis-ci.com/paritytech/parity-bitcoin
[doc-url]: https://paritytech.github.io/parity-bitcoin/pbtc/index.html

## Installing from source

Installing `pbtc` from source requires `rustc` and `cargo`.

Minimal supported version is `rustc 1.16.0 (30cf806ef 2017-03-10)`

#### Install rustc and cargo

Both `rustc` and `cargo` are a part of rust toolchain.

An easy way to install the stable binaries for Linux and Mac is to run this in your shell:

```
curl -sSf https://static.rust-lang.org/rustup.sh | sh
```

Windows binaries can be downloaded from [rust-lang website](https://www.rust-lang.org/en-US/downloads.html).

#### Install C and C++ compilers

You will need the cc and gcc compilers to build some of the dependencies

```
sudo apt-get update
sudo apt-get install build-essential
```

#### Clone and build pbtc

Now let's clone `pbtc` and enter it's directory

```
git clone https://github.com/paritytech/parity-bitcoin
cd parity-bitcoin
```

`pbtc` can be build in two modes. `--debug` and `--release`. Debug is the default

```
# builds pbtc in debug mode
cargo build -p pbtc
```

```
# builds pbtc in release mode
cargo build -p pbtc --release
```

`pbtc` is now available at either `./target/debug/pbtc` or `./target/release/pbtc`

## Installing the snap

In any of the [supported Linux distros](https://snapcraft.io/docs/core/install):

```
sudo snap install parity-bitcoin --edge
```

## Running tests

`pbtc` has internal unit tests and it conforms to external integration tests

#### Running unit tests

Assuming that repo is already cloned, we can run unit tests with this command:

```
./tools/test.sh
```

#### Running external integration tests

Running integration tests is automated, as regtests repo is one of the submodules. Let's download it first:

```
git submodule update --init
```

Now we can run them

```
./tools/regtests.sh
```

It's also possible to run regtests manually

```
# let's start pbtc in regtest compatible mode
./target/release/pbtc --regtest

# now in second shell window
cd $HOME
git clone https://github.com/TheBlueMatt/test-scripts
cd test-scripts
java -jar pull-tests-f56eec3.jar

```

## Going online

By default parity connects to bitcoind seednodes. Full list is [here](./pbtc/seednodes.rs)

To start syncing the mainnet, just start the client

```
./target/release/pbtc
```

To start syncing the testnet

```
./target/release/pbtc --testnet
```

To print syncing progress add `--print-to-console` flag

```
./target/release/pbtc --print-to-console
```

## Importing bitcoind database

It it is possible to import existing bitcoind database:

```
# where $BITCOIND_DB is path to your bitcoind database eg. "/Users/marek/Library/Application Support"
./target/release/pbtc --print-to-console import "$BITCOIND_DB/Bitcoin/blocks"
```

By default import verifies imported the blocks. You can disable this, by adding `--skip-verification flag.

```
./target/release/pbtc --print-to-console import "#BITCOIND_DB/Bitcoin/blocks" --skip-verification
```

## Command line interface

Full list of cli options, which is available under `pbtc --help`

```
pbtc 0.1.0
Parity Technologies <admin@parity.io>
Parity bitcoin client

USAGE:
    pbtc [FLAGS] [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help                Prints help information
        --no-jsonrpc          Disable the JSON-RPC API server
        --print-to-console    Send sync info to console
        --regtest             Use private network for regtest
        --testnet             Use the test network
    -V, --version             Prints version information

OPTIONS:
        --blocknotify <command>            Execute command when the best block changes (%s in cmd is replaced by block hash)
    -c, --connect <IP>                     Connect only to the specified node
    -d, --data-dir <PATH>                  Specify the database & configuration directory PATH
        --db-cache <SIZE>                  Sets db cache size
        --jsonrpc-apis <APIS>              Specify the APIs available through the JSONRPC interface. APIS is a comma-delimited list of API name.
        --jsonrpc-cors <URL>               Specify CORS header for JSON-RPC API responses
        --jsonrpc-hosts <HOSTS>            List of allowed Host header values
        --jsonrpc-interface <INTERFACE>    The hostname portion of the JSONRPC API server
        --jsonrpc-port <PORT>              The port portion of the JSONRPC API server
        --only-net <NET>                   Only connect to nodes in network <NET> (ipv4 or ipv6)
        --port <PORT>                      Listen for connections on PORT
    -s, --seednode <IP>                    Connect to a node to retrieve peer addresses, and disconnect

SUBCOMMANDS:
    help      Prints this message or the help of the given subcommand(s)
    import    Import blocks from bitcoin core database
```

## JSON-RPC

TODO

## Logging

This is a section only for dev / power users.

You can enable detailed client logging by setting env variable `RUST_LOG`

eg.

```
RUST_LOG=verification=info ./target/release/pbtc
```

`pbtc` started with this env variable will print all logs comming from `verification` module with verbosity `info` or higher

Available log levels:

- `error`
- `warn`
- `info`
- `debug`
- `trace`

It's also possible to start logging from multiple modules in the same time

```
RUST_LOG=sync=trace,p2p=trace,verification=trace,db=trace
```

*note* `RUST_LOG` does not work together with command line option `--print-to-console`

## Internal documentation

Once released, `pbtc` documentation will be available [here][doc-url]. Meanwhile it's only possible to build it locally:

```
cd parity-bitcoin
./tools/doc.sh
open target/doc/pbtc/index.html
```
