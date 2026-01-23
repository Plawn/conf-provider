
start-git:
    cargo run --bin server -- git --repo-url "https://git.blumana.app/infra/configuration.git" --branch "main" --username "c" --password "glpat-.0w1o703f1"

start-local:
    cargo run --bin server -- local --folder example

# Run all tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run only unit tests
test-unit:
    cargo test --test unit_tests

# Run nested config tests
test-nested:
    cargo test --test nested_configs

# Run e2e local server tests
test-e2e:
    cargo test --test e2e_local_server

# Run clippy linter
lint:
    cargo clippy

# Build the project
build:
    cargo build

# Build release
build-release:
    cargo build --release

# Get current version from Cargo.toml
version:
    @grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'

# Bump patch version (0.1.0 -> 0.1.1), commit, tag and push
bump-patch:
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    new_version="$major.$minor.$((patch + 1))"
    if git rev-parse "v$new_version" >/dev/null 2>&1; then
        echo "Error: tag v$new_version already exists" >&2
        exit 1
    fi
    sed -i.bak "s/^version = \"$current\"/version = \"$new_version\"/" Cargo.toml
    rm -f Cargo.toml.bak
    git add Cargo.toml
    git commit -m "chore: bump version to v$new_version"
    git tag "v$new_version"
    git push && git push --tags
    echo "Released v$new_version"

# Bump minor version (0.1.0 -> 0.2.0), commit, tag and push
bump-minor:
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    new_version="$major.$((minor + 1)).0"
    if git rev-parse "v$new_version" >/dev/null 2>&1; then
        echo "Error: tag v$new_version already exists" >&2
        exit 1
    fi
    sed -i.bak "s/^version = \"$current\"/version = \"$new_version\"/" Cargo.toml
    rm -f Cargo.toml.bak
    git add Cargo.toml
    git commit -m "chore: bump version to v$new_version"
    git tag "v$new_version"
    git push && git push --tags
    echo "Released v$new_version"

# Bump major version (0.1.0 -> 1.0.0), commit, tag and push
bump-major:
    #!/usr/bin/env bash
    set -euo pipefail
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    new_version="$((major + 1)).0.0"
    if git rev-parse "v$new_version" >/dev/null 2>&1; then
        echo "Error: tag v$new_version already exists" >&2
        exit 1
    fi
    sed -i.bak "s/^version = \"$current\"/version = \"$new_version\"/" Cargo.toml
    rm -f Cargo.toml.bak
    git add Cargo.toml
    git commit -m "chore: bump version to v$new_version"
    git tag "v$new_version"
    git push && git push --tags
    echo "Released v$new_version"