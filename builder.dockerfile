FROM debian:stretch as builder
RUN apt-get -qq update
RUN apt-get -y install curl build-essential libssl-dev pkg-config
RUN curl https://sh.rustup.rs -sSf > /tmp/rustup.sh && chmod 755 /tmp/rustup.sh && /tmp/rustup.sh -y
RUN mkdir /tmp/builder
COPY builder /tmp/builder
RUN cd /tmp/builder && /root/.cargo/bin/cargo build --release

FROM debian:stretch
MAINTAINER Samuel Tardieu <sam@rfc1149.net>
ENV DEBIAN_FRONTEND noninteractive
RUN apt-get -qq update
RUN apt-get -y dist-upgrade
RUN apt-get --no-install-recommends -y install \
      build-essential ca-certificates clang g++ \
      ccache autoconf automake libboost-all-dev \
      flex bison valgrind llvm-3.9-dev \
      python3-yaml python3-docopt libssl-dev \
      zlib1g-dev
COPY --from=builder /tmp/builder/target/release/builder /
ENTRYPOINT ["/builder"]
