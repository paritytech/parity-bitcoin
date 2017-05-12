FROM ubuntu:14.04
MAINTAINER Parity Technologies <devops@parity.io>
WORKDIR /build
#ENV for build TAG
ARG BUILD_TAG
ENV BUILD_TAG ${BUILD_TAG:-master}
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
        libssl-dev \
        pkg-config \
        libudev-dev && \
# install rustup
 curl https://sh.rustup.rs -sSf | sh -s -- -y && \
# rustup directory
 PATH=/root/.cargo/bin:$PATH && \
 # show tools
 rustc -vV && \
 cargo -V && \
 gcc -v && \
 g++ -v && \
# show backtraces
 RUST_BACKTRACE=1 && \
# build pbtc-ubuntu
 cd /build&&git clone https://github.com/paritytech/parity-bitcoin && \
        cd parity-bitcoin && \
	git pull&& \
	git checkout $BUILD_TAG && \
        cargo build --verbose --release && \
        strip /build/parity/target/release/pbtc && \
 file /build/parity/target/release/pbtc&&mkdir -p /pbtc-ubuntu&& \
 cp /build/parity/target/release/pbtc /pbtc-ubuntu/pbtc-ubuntu&& \
#cleanup Docker image
 rm -rf /root/.cargo&&rm -rf /root/.multirust&&rm -rf /root/.rustup&&rm -rf /build&&\
 apt-get purge -y  \
 g++ \
 build-essential \
 curl \
 git \
 file \
 binutils \
 libssl-dev \
 pkg-config \
 libudev-dev && \
 rm -rf /var/lib/apt/lists/*
# setup ENTRYPOINT
EXPOSE 8333 18333 8332 18332
ENTRYPOINT ["/pbtc-ubuntu/pbtc-ubuntu"]
