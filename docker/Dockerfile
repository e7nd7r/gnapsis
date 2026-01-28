# Extends Apache AGE with pgvector support for embeddings
# AGE 1.6.0 on PostgreSQL 17
FROM apache/age:release_PG17_1.6.0

# Install pgvector from the PostgreSQL apt repository
# Pinned to 0.8.0-1 to match PG17 in the base image
RUN apt-get update && apt-get install -y --no-install-recommends \
    postgresql-17-pgvector=0.8.0-1 \
    && rm -rf /var/lib/apt/lists/*
