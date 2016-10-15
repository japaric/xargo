FROM ubuntu:14.04
RUN dpkg --add-architecture i386 && apt-get update
RUN apt-get install -y --no-install-recommends \
    ca-certificates \
    cmake \
    gcc-multilib \
    libssh2-1-dev:i386 \
    libssl-dev:i386 \
    make
