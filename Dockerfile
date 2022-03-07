# Builder
FROM rust:latest AS builder

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev pkg-config libssl-dev
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

RUN cargo build --target x86_64-unknown-linux-musl --release

# Final image
FROM scratch

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /subql

# Copy our build
COPY --from=builder /subql/target/x86_64-unknown-linux-musl/release/subql-proxy .

# Use an unprivileged user.
USER subql:subql

ENTRYPOINT ["./subql-proxy"]