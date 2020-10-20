all: build run

build:
	cargo build

run:
	RUST_BACKTRACE=1 RUST_LOG=rori_discord_bot=info cargo run
