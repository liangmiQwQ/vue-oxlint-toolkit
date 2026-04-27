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
  -cargo shear --fix

update:
  cargo upgrade
  cargo update
  vp update major

test: 
  cargo test --all-features --workspace
  vp test

ready:
  git diff --exit-code --quiet
  just lint
  just fix
  just test
  git status
  git diff --exit-code --quiet

lint: 
  cargo shear
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  vp check

build:
  cargo build
  vpr build

bench:
  cargo bench -p benchmark

bump:
  cargo workspaces version -y -m "chore: release v%v" --no-individual-tags

