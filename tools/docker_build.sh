#!/bin/bash

##ARGUMENTS: 1. Docker target
set -e # fail on any error
set -u # treat unset variables as error

#if [ "$CI_COMMIT_REF_NAME" == "master" ];
#	then export DOCKER_BUILD_TAG="latest}";
#	else export DOCKER_BUILD_TAG=$CI_COMMIT_REF_NAME;
#fi
docker login -u $Docker_Hub_User_Parity -p $Docker_Hub_Pass_Parity

#echo "__________Docker TAG__________"
#echo $DOCKER_BUILD_TAG

echo "__________Docker build and push__________"
docker build --no-cache=true --tag parity/pbtc-ubuntu:latest -f docker/hub/Dockerfile .
docker push parity/pbtc-ubuntu:latest
docker logout
