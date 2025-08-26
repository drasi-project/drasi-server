# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

#!/bin/bash

GREEN="\033[32m"
RESET="\033[0m"

echo -e "${GREEN}\nRunning the Drasi Server with HTTP Source and Reaction...${RESET}"
# RUST_LOG=info cargo run --release -- -c./tests/http/http_example.yaml

# Make default logging off, enable only the server crate at info
# Normalize style to avoid ANSI codes so filtering works reliably
RUST_LOG_STYLE=never \
RUST_LOG="off,drasi_server=info" \
	cargo run --release -- -c./tests/http/http_example.yaml \
	| egrep '^(TRACE|DEBUG|INFO|WARN|ERROR)'
