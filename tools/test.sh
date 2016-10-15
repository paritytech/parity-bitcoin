#!/bin/bash

cargo test\
	-p db\
	-p ethcore-devtools\
	-p chain\
	-p bitcrypto\
	-p keys\
	-p message\
	-p p2p\
	-p primitives\
	-p script\
	-p serialization\
	-p pbtc
