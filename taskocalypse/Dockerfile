FROM node:22-bookworm-slim AS frontend
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY index.html tsconfig.json vite.config.ts ./
COPY src ./src
COPY public ./public
RUN npm run build

FROM rust:1-bookworm AS backend
WORKDIR /app
COPY server ./server
RUN cargo build --release --manifest-path server/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && useradd --create-home --uid 10001 town && mkdir /data && chown town:town /data
WORKDIR /app
COPY --from=backend /app/server/target/release/taskocalypse-server /usr/local/bin/taskocalypse-server
COPY --from=frontend /app/dist ./dist
ENV BIND_ADDRESS=0.0.0.0:8080 STATIC_DIR=/app/dist DATABASE_PATH=/data/taskocalypse.db
USER town
EXPOSE 8080
CMD ["taskocalypse-server"]
