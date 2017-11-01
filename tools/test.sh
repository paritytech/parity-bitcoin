#!/bin/bash

cargo test\
	-p bitcrypto\
	-p chain\
	-p db\
	-p import\
	-p keys\
	-p message\
	-p miner\
	-p network\
	-p pbtc\
	-p p2p\
	-p primitives\
	-p rpc\
	-p script\
	-p serialization\
	-p serialization_derive\
	-p sync\
	-p test-data\
	-p verification
