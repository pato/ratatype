#!/bin/bash
set -eu 

RUST_BACKTRACE=1 cargo run -- -d 10 2> stderr.log
