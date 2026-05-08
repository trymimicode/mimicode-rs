### File Structure

mimicode-rs/
├── Cargo.toml
└── src/
    ├── main.rs         ← CLI parsing, entry point
    ├── agent.rs        ← the turn loop
    ├── providers.rs    ← HTTP calls to Anthropic API
    └── types.rs        ← shared structs (Message, ContentBlock, etc.)