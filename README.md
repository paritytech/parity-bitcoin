# parity-bitcoin

The Parity Bitcoin client

[![Build Status][travis-image]][travis-url] [![Snap Status](https://build.snapcraft.io/badge/paritytech/parity-bitcoin.svg)](https://build.snapcraft.io/user/paritytech/parity-bitcoin)

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
        --jsonrpc-apis <APIS>              Specify the APIs available through the JSONRPC interface. APIS is a comma-delimited list of API name. Available APIs are blockchain, network, miner, raw.
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

#### Network

The Parity-bitcoin `network` interface.

##### addnode

Add the node.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "add"], "id":1 }' localhost:8332

Remove the node.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "remove"], "id":1 }' localhost:8332

Connect to the node.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "onetry"], "id":1 }' localhost:8332

##### getaddednodeinfo

Query info for all added nodes.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "id":"1", "method": "getaddednodeinfo", "params": [true] }' localhost:8332

Query info for the specified node.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "id":"1", "method": "getaddednodeinfo", "params": [true, "192.168.0.201"] }' localhost:8332

##### getconnectioncount

Get the peer count.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "id":"1", "method": "getconnectioncount", "params": [] }' localhost:8332

#### Blockchain

The Parity-bitcoin `blockchain` data interface.

##### getbestblockhash

Get hash of best block.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getbestblockhash", "params": [], "id":1 }' localhost:8332

##### getblockcount

Get height of best block.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getblockcount", "params": [], "id":1 }' localhost:8332

##### getblockhash

Get hash of block at given height.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getblockhash", "params": [0], "id":1 }' localhost:8332

##### getdifficulty

Get proof-of-work difficulty as a multiple of the minimum difficulty

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getdifficulty", "params": [], "id":1 }' localhost:8332

##### getblock

Get information on given block.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getblock", "params": ["000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"], "id":1 }' localhost:8332

##### gettxout

Get details about an unspent transaction output.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "gettxout", "params": ["4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b", 0], "id":1 }' localhost:8332

##### gettxoutsetinfo

Get statistics about the unspent transaction output set.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "gettxoutsetinfo", "params": [], "id":1 }' localhost:8332

#### Miner

The Parity-bitcoin `miner` data interface.

##### getblocktemplate

Get block template for mining.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getblocktemplate", "params": [{"capabilities": ["coinbasetxn", "workid", "coinbase/append"]}], "id":1 }' localhost:8332

#### Raw

The Parity-bitcoin `raw` data interface.


##### getrawtransaction

Return the raw transaction data.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "getrawtransaction", "params": ["4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"], "id":1 }' localhost:8332

##### decoderawtransaction

Return an object representing the serialized, hex-encoded transaction.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "decoderawtransaction", "params": ["01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"], "id":1 }' localhost:8332

##### createrawtransaction

Create a transaction spending the given inputs and creating new outputs.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "createrawtransaction", "params": [[{"txid":"4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b","vout":0}],{"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa":0.01}], "id":1 }' localhost:8332

##### sendrawtransaction

Adds transaction to the memory pool && relays it to the peers.

    curl -H 'content-type: application/json' --data-binary '{"jsonrpc": "2.0", "method": "sendrawtransaction", "params": ["01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"], "id":1 }' localhost:8332

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
