set windows-shell := ["powershell"]
set shell := ["bash", "-cu"]

_default:
  @just --list -u


init:
  cargo install cargo-binstall
  cargo binstall cargo-insta cargo-shear cargo-workspaces cargo-edit cargo-llvm-cov dprint -y
  just install-hook
  
fmt: 
  cargo fmt --all -- --emit=files
  dprint fmt

install-hook:
  echo -e "#!/bin/sh\njust fmt" > .git/hooks/pre-commit
  chmod +x .git/hooks/pre-commit

fix:
  just fmt
  cargo fix --allow-dirty --allow-staged
  -cargo shear --fix

update:
  cargo upgrade
  cargo update

test: 
  cargo test --all-features --workspace

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

build:
  cargo build

bench:
  cargo bench -p benchmark

bump:
  cargo workspaces version -y -m "chore: release v%v" --no-individual-tags

coverage:
  cargo llvm-cov --all-features --workspace
