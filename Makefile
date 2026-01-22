.PHONY: reset migrate seed run check fmt clippy test

reset:
	./scripts/reset_dev.sh

migrate:
	./scripts/migrate.sh

seed:
	./scripts/seed_dev.sh

run:
	cargo run --bin DCMS-Rust-Server


check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

test:
	cargo test

clean:
	cargo clean
seed-demo:
		psql "$$DATABASE_URL" -f scripts/seed_demo_data.sql

test-auth:
		./scripts/test_auth_roles.sh

test-patient:
		./scripts/test_patient_permissions.sh

test-sms:
		./scripts/test_sms_history.sh

