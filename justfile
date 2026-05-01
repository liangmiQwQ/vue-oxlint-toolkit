set windows-shell := ["powershell"]
set shell := ["bash", "-cu"]

_default:
  @just --list -u


init:
  cargo binstall cargo-insta cargo-shear cargo-workspaces cargo-edit -y
  vp install
  
fmt: 
  cargo fmt --all -- --emit=files

fix:
  just fmt
  cargo fix --allow-dirty --allow-staged
  vp check --fix
  # -cargo shear --fix

update:
  cargo upgrade
  cargo update
  vp update major

test: 
  just build
  cargo test --all-features --workspace
  vp test

ready:
  git diff --exit-code --quiet
  just lint
  just build
  just test
  git status
  git diff --exit-code --quiet

lint: 
  # cargo shear
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo fmt --all -- --check
  vp check

build:
  cargo build
  vpr build

bench:
  cargo bench -p benchmark

bump TYPE:
  just main
  node -p "require('semver').valid('{{ TYPE }}') || (console.error('Invalid version'), process.exit(1))"
  vpx bumpp --no-commit -y --release -r {{ TYPE }}
  cargo workspaces version --no-git-commit -y custom {{ TYPE }}
  just build
  git add .
  git commit -m "chore: release v{{ TYPE }}"
  git tag v{{ TYPE }}
  git push origin main v{{ TYPE }}

main:
  git checkout main
  git pull origin main
