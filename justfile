build:
    flatpak-builder --user --install --force-clean build io.github.didley.CamOverlay.yml

run:
    flatpak run io.github.didley.CamOverlay

dev:
    yq '.modules[0].sources[0] = {"type": "dir", "path": "."}' \
        io.github.didley.CamOverlay.yml > /tmp/camoverlay-dev.yml
    flatpak-builder --user --install --force-clean build /tmp/camoverlay-dev.yml
    flatpak run io.github.didley.CamOverlay

lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.didley.CamOverlay.yml
