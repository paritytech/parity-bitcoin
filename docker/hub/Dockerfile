# This Dockerfile uses Docker Multi-Stage Builds
# See https://docs.docker.com/engine/userguide/eng-image/multistage-build/

# Build image
FROM rust:1.23-jessie AS build

#ENV for build TAG
ARG BUILD_TAG
ENV BUILD_TAG=${BUILD_TAG:-master} \
    PATH=/root/.cargo/bin:$PATH \
    RUST_BACKTRACE=1

WORKDIR /build
RUN echo $BUILD_TAG

# install tools and dependencies
RUN apt-get update && \
        apt-get install -y --force-yes --no-install-recommends \
        g++ \
        build-essential \
        curl \
        git \
        file \
        binutils \
        ca-certificates \
        libssl-dev \
        pkg-config \
        libudev-dev

# build pbtc-ubuntu
RUN git clone https://github.com/paritytech/parity-bitcoin

WORKDIR /build/parity-bitcoin
RUN git pull
RUN git checkout $BUILD_TAG
RUN cargo build --verbose --release
RUN strip /build/parity-bitcoin/target/release/pbtc
RUN file /build/parity-bitcoin/target/release/pbtc

# Runtime image, copies pbtc artifact from build image
FROM ubuntu:16.04 AS run
LABEL maintainer "Parity Technologies <devops@parity.io>"

WORKDIR /pbtc-ubuntu
COPY --from=build /build/parity-bitcoin/target/release/pbtc/ /pbtc-ubuntu/pbtc-ubuntu

# setup ENTRYPOINT
EXPOSE 8333 18333 8332 18332
ENTRYPOINT ["/pbtc-ubuntu/pbtc-ubuntu"]
