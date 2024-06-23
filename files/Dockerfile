# Use the official Rust image as the base image
FROM rust:1.75-slim as builder

# Install dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev libsqlite3-dev && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/matrix_calendar_bot

# Copy the Cargo.toml and Cargo.lock files
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src ./src

# Install dependencies and build the application
RUN cargo build --release

# Use a minimal image for the final build
FROM ubuntu:22.04

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y libsqlite3-0 libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/matrix_calendar_bot

# Copy the built binary from the build stage
COPY --from=0 /usr/src/matrix_calendar_bot/target/release/matrix_calendar_bot .

# Copy the .env file
COPY .env .env

# Set the environment variable for logging
ENV RUST_LOG=error

# Run the application
CMD ["./matrix_calendar_bot"]
