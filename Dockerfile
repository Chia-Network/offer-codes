FROM rust:bookworm AS builder
WORKDIR /app
COPY . .
RUN apt update && apt install -y cmake
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/offercode /app/offer-codes
WORKDIR /app
ENTRYPOINT ["/app/offer-codes"]
