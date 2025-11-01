#!/bin/sh
set -eu

old_dir="$ROOT"/.allium/screenshots
new_dir="$ROOT"/Saves/CurrentProfile/screenshots

if [ -d "$old_dir" ]; then
    mkdir -p "$new_dir"
    if [ -n "$(ls -A "$old_dir" 2>/dev/null)" ]; then
        cp -a "$old_dir"/. "$new_dir"/
    fi
    rm -rf "$old_dir"
fi

old_state_dir="$ROOT"/.allium/state
new_state_dir="$ROOT"/Saves/CurrentProfile/allium/state

mkdir -p "$new_state_dir"

if [ -d "$old_state_dir" ]; then
    if [ -n "$(ls -A "$old_state_dir" 2>/dev/null)" ]; then
        cp -a "$old_state_dir"/. "$new_state_dir"/
    fi
    rm -rf "$old_state_dir"
fi
