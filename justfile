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

# Run all checks (lib, examples, tests, docs)
check-all:
	cargo xtask check-all

# Generate video frames data
video-frames:
	cargo xtask video-frames-gen > video_frames_data.rs

# Build an example for Pico 2 (ARM)
example name:
	cargo xtask example {{name}} --board pico2 --arch arm

# Build an example for Pico 2 (ARM) with WiFi
example-wifi name:
	cargo xtask example {{name}} --board pico2 --arch arm --wifi

# Build an example for Pico 1 with WiFi
example-pico1 name:
	cargo xtask example {{name}} --board pico1 --arch arm --wifi

# Build UF2 file for Pico 2 (ARM)
uf2 name:
	cargo xtask uf2 {{name}} --board pico2 --arch arm

# Build UF2 file for Pico 2 (ARM) with WiFi
uf2-wifi name:
	cargo xtask uf2 {{name}} --board pico2 --arch arm --wifi