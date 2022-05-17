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

RUN ls
RUN ls
RUN cargo install --path /build
RUN rm -rf /build

ENV IMAGE_DB_PORT=5001
# Can configure per-user later
ENV MAX_FILE_SIZE_KB = 200000
ENV DATABASE_URL = "images.db"
CMD ["image_db"]