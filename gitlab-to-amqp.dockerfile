FROM debian:stretch as builder
RUN apt-get -qq update
RUN apt-get -y install curl build-essential libssl-dev pkg-config cmake zlib1g-dev
RUN curl https://sh.rustup.rs -sSf > /tmp/rustup.sh && chmod 755 /tmp/rustup.sh && /tmp/rustup.sh -y
RUN mkdir /tmp/builder
COPY . /tmp/builder
RUN cd /tmp/builder/gitlab-to-amqp && /root/.cargo/bin/cargo build --release

FROM debian:stretch
MAINTAINER Samuel Tardieu <sam@rfc1149.net>
ENV DEBIAN_FRONTEND noninteractive
RUN apt-get -qq update
COPY --from=builder /tmp/builder/target/release/gitlab-to-amqp /
EXPOSE 80
ENTRYPOINT ["/gitlab-to-amqp"]
CMD ["/config.yml"]
