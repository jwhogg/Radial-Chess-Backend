# Start with a Rust base image
FROM rust:1.78-bullseye as builder

# Set up working directory
WORKDIR /app

# Copy your Rust source code
COPY . .

# Build the Rust project in release mode
RUN cargo build --release

# Use a smaller image for the runtime
FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder
COPY --from=builder /app/target/release/radial_chess /usr/local/bin/radial_chess

# Expose ports (e.g., 8080 for HTTP and 9000 for WebSocket)
EXPOSE 8080 9000

# Command to run your server
CMD ["radial_chess"]
