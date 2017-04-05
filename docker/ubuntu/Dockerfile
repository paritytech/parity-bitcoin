FROM ubuntu:14.04
WORKDIR /build
ADD . /build/pbtc

# install tools and dependencies
RUN apt-get update && \
        apt-get install -y \
        g++ \
        build-essential \
        curl \
        git \
        file \
        binutils \
        libssl-dev \
        pkg-config \
        libudev-dev

# install rustup
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

# rustup directory
ENV PATH /root/.cargo/bin:$PATH

# show backtraces
ENV RUST_BACKTRACE 1

# show tools
RUN rustc -vV && \
cargo -V && \
gcc -v &&\
g++ -v

# build pbtc
RUN cd pbtc && cargo build --release --verbose

RUN file /build/pbtc/target/release/pbtc

EXPOSE 8333 18333 8332 18332
ENTRYPOINT ["/build/pbtc/target/release/pbtc"]
