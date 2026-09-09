FROM rust:1.93.1-bookworm AS builder

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cd website && npm ci && npx tailwindcss -i src/styles/input.css -o static/output.css --minify

ENV SQLX_OFFLINE=true
RUN cargo build -p website --release && \
    strip target/release/website

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /app/target/release/website /website
COPY --from=builder /app/website/static /static

ENV STATIC_DIR=/static
EXPOSE 80
ENTRYPOINT ["/website"]
