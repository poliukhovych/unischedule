.PHONY: all build run test fmt lint docker
all: build
build:
	cargo build --release
run:
	UNISCHEDULE__SERVER__PORT=8080 cargo run -p api
fmt:
	cargo fmt --all
lint:
	cargo clippy --all-targets --all-features -- -D warnings
test:
	cargo test --workspace --all-features
docker:
	docker build -f docker/Dockerfile -t unischedule:dev .
