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
RUN apt-get update -y
RUN apt-get install -y libcurl4-openssl-dev

COPY --from=builder /build/target/release/image_db /usr/local/bin

WORKDIR /

ENV IMAGE_DB_ADDR=0.0.0.0:8081
ENV DATA_DIR = "/data/db/"
ENV DATABASE_URL = "/data/db/images.db"
ENV UPLOAD_DIR = "/data/uploaded_images/"
ENV MOUNTED_IMAGE_DIR = ""

# Can configure per-user later
ENV MAX_FILE_SIZE_KB = 200000

CMD ["/usr/local/bin/image_db"]