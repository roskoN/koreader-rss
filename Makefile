SHELL := /bin/sh

TARGET ?= armv7-unknown-linux-musleabihf
PACKAGE_DIR ?= dist/rssreader.koplugin

.PHONY: test build build-arm test-arm package deploy deploy-backend deploy-plugin kindle-test logs clean

test:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

build:
	cargo build --release --package rss-backend

build-arm:
	cross build --release --target "$(TARGET)" --package rss-backend

test-arm: build-arm
	TARGET="$(TARGET)" ./scripts/test-arm.sh

package: build-arm
	TARGET="$(TARGET)" PACKAGE_DIR="$(PACKAGE_DIR)" ./scripts/package.sh

deploy: package
	PACKAGE_DIR="$(PACKAGE_DIR)" ./scripts/deploy.sh all

deploy-backend: build-arm
	TARGET="$(TARGET)" ./scripts/deploy.sh backend

deploy-plugin:
	./scripts/deploy.sh plugin

kindle-test:
	./scripts/kindle-test.sh

logs:
	./scripts/kindle-logs.sh

clean:
	cargo clean
	rm -rf dist
