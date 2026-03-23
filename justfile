build:
    flatpak-builder --user --install --force-clean build io.github.didley.CamOverlay.yml

run:
    flatpak run io.github.didley.CamOverlay

dev: build run

lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.didley.CamOverlay.yml
