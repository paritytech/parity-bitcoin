#!/bin/bash

cargo doc --no-deps\
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
	-p verification
