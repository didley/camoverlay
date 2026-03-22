project('camoverlay',
  'rust',
  version: '0.1.0',
  license: 'GPL-3.0-or-later',
  meson_version: '>= 1.0',
)

gnome = import('gnome')
cargo = find_program('cargo', required: true)

app_id = 'io.github.didley.CamOverlay'
prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
datadir = prefix / get_option('datadir')

subdir('data')

# Generate config.rs from template
config_rs = configure_file(
  input: 'src/config.rs.in',
  output: 'config.rs',
  configuration: {
    'APP_ID': app_id,
    'VERSION': meson.project_version(),
  },
)
run_command(
  'cp',
  config_rs,
  meson.project_source_root() / 'src' / 'config.rs',
  check: false,
)

cargo_options = ['--manifest-path', meson.project_source_root() / 'Cargo.toml']
cargo_options += ['--target-dir', meson.project_build_root() / 'target']

if get_option('buildtype') == 'release'
  cargo_options += ['--release']
  rust_target = 'release'
else
  rust_target = 'debug'
endif

cargo_env = environment()
cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo-home')

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: 'camoverlay',
  console: true,
  env: cargo_env,
  command: [
    find_program('build-aux/cargo.sh'),
    meson.project_source_root(),
    meson.project_build_root(),
    rust_target,
    'camoverlay',
  ],
  install: true,
  install_dir: bindir,
)
