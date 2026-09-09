FROM rust:1.93.1-bookworm AS builder

WORKDIR /app
COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build -p api_server --release && \
    strip target/release/api_server

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /app/target/release/api_server /api_server

EXPOSE 9000
ENTRYPOINT ["/api_server"]
