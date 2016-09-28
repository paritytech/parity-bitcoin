#!/bin/bash

cargo test\
	-p chain\
	-p bitcrypto\
	-p keys\
	-p net\
	-p p2p\
	-p primitives\
	-p script\
	-p serialization\
	-p pbtc
