#!/usr/bin/env sh

for crate in crates/lks-lsp crates/lks crates/ferry
do
    cargo install --path "$crate"
done