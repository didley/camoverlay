build:
    flatpak-builder --user --install --force-clean build io.github.didley.CamOverlay.yml

run:
    flatpak run io.github.didley.CamOverlay

dev:
    flatpak-builder --user --install --force-clean --disable-cache build io.github.didley.CamOverlay.yml
    flatpak run io.github.didley.CamOverlay

lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.didley.CamOverlay.yml

# Bumps version, commit, and tag, with optoinal release notes 
# Usage: just release 0.2.0 "Added new feature"
release version notes="":
    @echo "Bumping version to {{version}}..."
    sed -i 's/^version = ".*"/version = "{{version}}"/' Cargo.toml
    cargo generate-lockfile
    sed -i "s/version: '.*'/version: '{{version}}'/" meson.build
    sed -i '/<releases>/a\    <release version="{{version}}" date="'"$(date +%Y-%m-%d)"'">\n      <description>\n        <p>{{ if notes != "" { notes } else { "Release " + version + "." } }}</p>\n      </description>\n    </release>' data/io.github.didley.CamOverlay.metainfo.xml
    @echo "Committing and tagging v{{version}}..."
    git add Cargo.toml Cargo.lock meson.build data/io.github.didley.CamOverlay.metainfo.xml
    git commit -m "Release v{{version}}"
    git tag "v{{version}}"
    @echo "Done! Run 'git push && git push --tags' to publish."
