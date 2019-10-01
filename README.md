# The Parity Bitcoin client.

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

Minimal supported version is `rustc 1.23.0 (766bd11c8 2018-01-01)`

#### Install rustc and cargo

Both `rustc` and `cargo` are a part of rust tool-chain.

An easy way to install the stable binaries for Linux and Mac is to run this in your shell:

```
curl -sSf https://static.rust-lang.org/rustup.sh | sh
```

Windows binaries can be downloaded from [rust-lang website](https://www.rust-lang.org/en-US/downloads.html).

#### Install C and C++ compilers

You will need the cc and gcc compilers to build some of the dependencies.

```
sudo apt-get update
sudo apt-get install build-essential
```

#### Clone and build pbtc

Now let's clone `pbtc` and enter it's directory:

```
git clone https://github.com/paritytech/parity-bitcoin
cd parity-bitcoin
```

`pbtc` can be build in two modes. `--debug` and `--release`. Debug is the default.

```
# builds pbtc in debug mode
cargo build -p pbtc
```

```
# builds pbtc in release mode
cargo build -p pbtc --release
```

`pbtc` is now available at either `./target/debug/pbtc` or `./target/release/pbtc`.

## Installing the snap

In any of the [supported Linux distros](https://snapcraft.io/docs/core/install):

```
sudo snap install parity-bitcoin --edge
```

## Running tests

`pbtc` has internal unit tests and it conforms to external integration tests.

#### Running unit tests

Assuming that repository is already cloned, we can run unit tests with this command:

```
cargo test --all
```

#### Running external integration tests

Running integration tests is automated, as the regtests repository is one of the submodules. Let's download it first:

```
git submodule update --init
```

Now we can run them using the command:

```
./tools/regtests.sh
```

It is also possible to run regtests manually:

```
# let's start pbtc in regtest compatible mode
./target/release/pbtc --btc --regtest

# now in second shell window
cd $HOME
git clone https://github.com/TheBlueMatt/test-scripts
cd test-scripts
java -jar pull-tests-f56eec3.jar

```

## Going online

By default parity connects to bitcoind-seednodes. Full list is available [here](./pbtc/seednodes.rs).

Before starting synchronization, you must decide - which fork to follow - Bitcoin Core (`--btc` flag) or Bitcoin Cash (`--bch` flag). On next start, passing the same flag is optional, as the database is already bound to selected fork and won't be synchronized using other verification rules.

To start syncing the main network, just start the client, passing selected fork flag. For example:

```
./target/release/pbtc --btc
```

To start syncing the testnet:

```
./target/release/pbtc --btc --testnet
```

To not print any syncing progress add `--quiet` flag:

```
./target/release/pbtc --btc --quiet
```

## Importing bitcoind database

It is possible to import existing `bitcoind` database:

```
# where $BITCOIND_DB is path to your bitcoind database, e.g., "/Users/user/Library/Application Support"
./target/release/pbtc import "$BITCOIND_DB/Bitcoin/blocks"
```

By default import verifies imported the blocks. You can disable this, by adding `--verification-level==none` flag.

```
./target/release/pbtc import "#BITCOIND_DB/Bitcoin/blocks" --btc --skip-verification
```

## Command line interface

Full list of CLI options, which is available under `pbtc --help`:

```
pbtc 0.1.0
Parity Technologies <info@parity.io>
Parity Bitcoin client

USAGE:
    pbtc [FLAGS] [OPTIONS] [SUBCOMMAND]

FLAGS:
        --bch             Use Bitcoin Cash verification rules (BCH).
        --btc             Use Bitcoin Core verification rules (BTC).
    -h, --help            Prints help information
        --no-jsonrpc      Disable the JSON-RPC API server.
    -q, --quiet           Do not show any synchronization information in the console.
        --regtest         Use a private network for regression tests.
        --testnet         Use the test network (Testnet3).
    -V, --version         Prints version information

OPTIONS:
        --blocknotify <COMMAND>            Execute COMMAND when the best block changes (%s in COMMAND is replaced by the block hash).
    -c, --connect <IP>                     Connect only to the specified node.
    -d, --data-dir <PATH>                  Specify the database and configuration directory PATH.
        --db-cache <SIZE>                  Sets the database cache size.
        --jsonrpc-apis <APIS>              Specify the APIs available through the JSONRPC interface. APIS is a comma-delimited list of API names.
        --jsonrpc-cors <URL>               Specify CORS header for JSON-RPC API responses.
        --jsonrpc-hosts <HOSTS>            List of allowed Host header values.
        --jsonrpc-interface <INTERFACE>    The hostname portion of the JSONRPC API server.
        --jsonrpc-port <PORT>              Specify the PORT for the JSONRPC API server.
        --only-net <NET>                   Only connect to nodes in network version <NET> (ipv4 or ipv6).
        --port <PORT>                      Listen for connections on PORT.
    -s, --seednode <IP>                    Connect to a seed-node to retrieve peer addresses, and disconnect.
        --verification-edge <BLOCK>        Non-default verification-level is applied until a block with given hash is met.
        --verification-level <LEVEL>       Sets the Blocks verification level to full (default), header (scripts are not verified), or none (no verification at all).

SUBCOMMANDS:
    help        Prints this message or the help of the given subcommand(s)
    import      Import blocks from a Bitcoin Core database.
    rollback    Rollback the database to given canonical-chain block.
```

## JSON-RPC

The JSON-RPC interface is served on port :8332 for mainnet and :18332 for testnet unless you specified otherwise. So if you are using testnet, you will need to change the port in the sample curl requests shown below.

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

This is a section only for developers and power users.

You can enable detailed client logging by setting the environment variable `RUST_LOG`, e.g.,

```
RUST_LOG=verification=info ./target/release/pbtc --btc
```

`pbtc` started with this environment variable will print all logs coming from `verification` module with verbosity `info` or higher. Available log levels are:

- `error`
- `warn`
- `info`
- `debug`
- `trace`

It's also possible to start logging from multiple modules in the same time:

```
RUST_LOG=sync=trace,p2p=trace,verification=trace,db=trace ./target/release/pbtc --btc
```

## Internal documentation

Once released, `pbtc` documentation will be available [here][doc-url]. Meanwhile it's only possible to build it locally:

```
cd parity-bitcoin
./tools/doc.sh
open target/doc/pbtc/index.html
```
