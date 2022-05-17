FROM lukemathwalker/cargo-chef:latest AS chef

WORKDIR build

FROM chef AS planner
COPY ./src src
COPY ./Cargo.toml Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /build/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

COPY ./src src
COPY ./Cargo.toml Cargo.toml
COPY ./reference.db reference.db
COPY ./.env .env
RUN cargo build --release

FROM debian:bullseye-slim AS runtime
COPY --from=builder /build/target/release/image_db /usr/local/bin

ENV IMAGE_DB_PORT=8081
# Can configure per-user later
ENV MAX_FILE_SIZE_KB = 200000
ENV DATABASE_URL = "images.db"
CMD ["/usr/local/bin/image_db"]