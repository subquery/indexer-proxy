# Builder
FROM rust:1.59.0 AS builder

RUN update-ca-certificates

# Create appuser
ENV USER=subql
ENV UID=10001

RUN adduser \
  --disabled-password \
  --gecos "" \
  --home "/nonexistent" \
  --shell "/sbin/nologin" \
  --no-create-home \
  --uid "${UID}" \
  "${USER}"


WORKDIR /subql

COPY ./ .

RUN cargo build --release

# Final image
FROM debian:buster-slim

WORKDIR /subql

# Copy our build
COPY --from=builder /subql/target/release/subql-proxy .

# Use an unprivileged user.
RUN groupadd --gid 10001 subql && \
    useradd  --home-dir /subql \
             --create-home \
             --shell /bin/bash \
             --gid subql \
             --groups subql \
             --uid 10000 subql
RUN mkdir -p /subql/.local/share && \
	mkdir /subql/data && \
	chown -R subql:subql /subql && \
	ln -s /subql/data /subql/.local/share
USER subql:subql

ENTRYPOINT ["./subql-proxy"]
