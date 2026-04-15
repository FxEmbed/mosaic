FROM rust:alpine3.22 as builder
WORKDIR /usr/src/mosaic

# Install build dependencies for musl libc (what Alpine uses)
RUN apk add --no-cache musl-dev

COPY . .
RUN cargo install --path .

FROM alpine:3.22
COPY --from=builder /usr/local/cargo/bin/mosaic /usr/local/bin/mosaic

CMD ["mosaic"]

EXPOSE 3030
