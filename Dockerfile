# Linux test image for git-wt.
# Builds the crate and runs both the unit tests (`cargo test`) and the live
# end-to-end suite (`test-mac.sh`) on Debian Linux.
FROM rust:slim

# test-mac.sh drives a real repo, so git is required. ca-certificates keeps any
# https git ops happy.
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# git-wt shells out to `git`; give it an identity so commits in the test repo
# don't fail on a bare container.
RUN git config --global user.email "test@example.com" \
 && git config --global user.name  "Test" \
 && git config --global init.defaultBranch main

WORKDIR /work
COPY . .

# Warm the build cache at image-build time so `docker run` is fast to iterate.
RUN cargo build --release

# Default: unit tests, then the live suite.
CMD ["bash", "-c", "cargo test --release && ./test-mac.sh"]
