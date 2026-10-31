FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/webrtc-mta-client /app/webrtc-mta-client
COPY --from=builder /app/static /app/static
ENV PORT=3000
EXPOSE 3000
CMD ["./webrtc-mta-client"]
