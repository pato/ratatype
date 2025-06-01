#!/bin/bash
set -eu 

RUST_BACKTRACE=1 cargo run 2> stderr.log
