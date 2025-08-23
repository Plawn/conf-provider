

start-git:
    cargo run --bin server -- git --repo-url "https://github.com/Plawn/configuration.git" --branch "main"

start-local:
    cargo run --bin server -- local --folder example