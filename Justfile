

start-git:
    cargo run --bin server -- git --repo-url "https://git.blumana.app/infra/configuration.git" --branch "main" --username plawn --password "apcg461M"

start-local:
    cargo run --bin server -- local --folder example