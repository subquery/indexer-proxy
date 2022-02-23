# Rust as the base image
FROM rust:1.54

RUN USER=root cargo new --bin subql-proxy
WORKDIR /subql-proxy

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release
RUN rm src/*.rs

COPY ./src ./src

RUN rm -r ./target/release/deps
RUN cargo install --path .

CMD ["subql-proxy"]