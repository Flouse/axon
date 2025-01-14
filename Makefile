VERBOSE := $(if ${CI},--verbose,)

COMMIT := $(shell git rev-parse --short HEAD)

CARGO := cargo

test:
	${CARGO} test ${VERBOSE} --all -- --nocapture

# Since Axon uses global variables, run all tests in one process may cause unexpected errors.
test-in-separate-processes:
	cargo nextest run --workspace --no-fail-fast --hide-progress-bar --failure-output final

doc:
	cargo doc --all --no-deps

doc-deps:
	cargo doc --all

check:
	${CARGO} check ${VERBOSE} --all

build:
	${CARGO} build ${VERBOSE} --release

check-fmt:
	cargo +nightly fmt ${VERBOSE} --all -- --check

fmt:
	cargo +nightly fmt ${VERBOSE} --all

clippy:
	${CARGO} clippy ${VERBOSE} --all --all-targets --all-features -- \
		-D warnings -D clippy::clone_on_ref_ptr -D clippy::enum_glob_use

sort:
	cargo sort -gw

check-sort:
	cargo sort -gwc

ci: check-fmt clippy test

info:
	date
	pwd
	env

# For counting lines of code
stats:
	@cargo count --version || cargo +nightly install --git https://github.com/kbknapp/cargo-count
	@cargo count --separator , --unsafe-statistics

# Use cargo-audit to audit Cargo.lock for crates with security vulnerabilities
# expecting to see "Success No vulnerable packages found"
security-audit:
	@cargo audit --version || cargo install cargo-audit
	@cargo audit

crosschain-test:
	cd builtin-contract/crosschain \
		&& yarn --from-lock-file && yarn run compile && yarn run test

crosschain-genesis-deploy:
	cd builtin-contract/crosschain && yarn run deploy

unit-test: test crosschain-test

schema:
	make -C core/cross-client/ schema

.PHONY: build prod prod-test
.PHONY: fmt test clippy doc doc-deps doc-api check stats
.PHONY: ci info security-audit
