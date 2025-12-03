
start-git:
    cargo run --bin server -- git --repo-url "https://git.blumana.app/infra/configuration.git" --branch "main" --username "<>" --password "<>"

start-local:
    cargo run --bin server -- local --folder example

# Run all tests
test:
    cargo +nightly test

# Run tests with output
test-verbose:
    cargo +nightly test -- --nocapture

# Run only unit tests
test-unit:
    cargo +nightly test --test unit_tests

# Run nested config tests
test-nested:
    cargo +nightly test --test nested_configs

# Run e2e local server tests
test-e2e:
    cargo +nightly test --test e2e_local_server

# Run clippy linter
lint:
    cargo +nightly clippy

# Build the project
build:
    cargo +nightly build

# Build release
build-release:
    cargo +nightly build --release