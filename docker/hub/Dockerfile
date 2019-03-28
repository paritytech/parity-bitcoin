FROM ubuntu:xenial
LABEL MAINTAINER="Parity Technologies <devops-team@parity.io>"

# install tools and dependencies
RUN apt update && apt install -y --no-install-recommends openssl

# show backtraces
ENV RUST_BACKTRACE 1

# cleanup Docker image
RUN apt autoremove -y \
  && apt clean -y \
  && rm -rf /tmp/* /var/tmp/* /var/lib/apt/lists/*

RUN groupadd -g 1000 pbtc-ubuntu \
  && useradd -m -u 1000 -g pbtc-ubuntu -s /bin/sh pbtc-ubuntu

WORKDIR /home/pbtc-ubuntu

# add parity-ethereum to docker image
COPY artifacts/$CARGO_TARGET/pbtc /bin/pbtc-ubuntu

COPY scripts/docker/hub/check_sync.sh /check_sync.sh

# switch to user parity here
USER pbtc-ubuntu

# setup ENTRYPOINT
EXPOSE 8333 18333 8332 18332
ENTRYPOINT ["/bin/pbtc-ubuntu"]
