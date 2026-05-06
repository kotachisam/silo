# silo

Ephemeral GPU rental orchestration. Personal CLI wrapping vast.ai (and eventually
others) for the spin-up / SSH / tunnel / destroy loop.

## Install

`cargo install --path .`

## Setup

`export VAST_API_KEY="your-key-from-https://cloud.vast.ai/manage-keys/"`

## Use

`silo search` — default filters: 1 GPU, 90+ GB VRAM, 200+ GB disk, US, reliability ≥ 0.99
`silo search --vram 180 --disk 500` — override
`silo up <offer_id> --boot ~/bin/boot.sh`
`silo status` — poll until state == "running"
`silo ssh` — interactive shell
`silo ssh -- ollama list` — one-shot remote command
`silo tunnel 11434` — forward localhost:11434
`silo down` — destroy

State lives at `~/Library/Application Support/silo/active.json`.
