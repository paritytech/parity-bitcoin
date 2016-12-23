#!/bin/bash

cargo test\
	-p bitcrypto\
	-p chain\
	-p db\
	-p ethcore-devtools\
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
	-p sync\
	-p test-data\
	-p verification
