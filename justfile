build:
    flatpak-builder --user --install --force-clean build io.github.didley.CamOverlay.yml

run:
    flatpak run io.github.didley.CamOverlay

dev:
    flatpak-builder --user --install --force-clean --disable-cache build io.github.didley.CamOverlay.yml
    flatpak run io.github.didley.CamOverlay

lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.didley.CamOverlay.yml
