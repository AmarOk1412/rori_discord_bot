all: build run

build:
	cargo build

run:
	RUST_BACKTRACE=1 RUST_LOG=info cargo run
