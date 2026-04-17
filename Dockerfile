FROM rust:alpine3.22 AS builder
WORKDIR /usr/src/mosaic

# musl headers + C toolchain (make, gcc): required by tikv-jemalloc-sys
RUN apk add --no-cache musl-dev build-base

COPY . .
RUN cargo install --path .

FROM alpine:3.22
COPY --from=builder /usr/local/cargo/bin/mosaic /usr/local/bin/mosaic

CMD ["mosaic"]

EXPOSE 3030
