open-docs:
	./scripts/open-docs.sh

gather:
	./scripts/gather.sh

attach-probe:
	./scripts/attach-probe.sh

regenerate-text-pngs:
	./scripts/regenerate-text-pngs.sh
doc-tests:
	cargo test --doc --target thumbv8m.main-none-eabihf --features pico2,wifi,arm --no-default-features