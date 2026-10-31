# Taskocalypse

A cozy, interactive 3D town that turns digital chores into small quests.

**Frontend:** React, TypeScript, Three.js, Vite. **Backend:** Rust, Axum, SQLite.

Explore Serenity Valley to discover keepsakes, earn XP, and follow the local-time day/night cycle. The 3D town is loaded on demand so sign-in and the rest of the interface can appear sooner.

See [the complete setup guide](public/SETUP.md) for local startup, Google OAuth, Docker deployment, and the supported feature boundaries.

```sh
npm ci
npm run build
# Configure .env from .env.example first
cargo run --manifest-path server/Cargo.toml
```

Open http://localhost:8080. Real accounts require `OTP_API_KEY` and `OTP_FROM` for verification emails. Real Gmail additionally requires your Google OAuth project. After OTP verification, automatic inbox syncing runs within a fixed three-hour activation; replies and archiving remain manual. The labelled demo works without credentials.

On this Windows workspace, `powershell -ExecutionPolicy Bypass -File .\start.ps1` uses the project-local Rust toolchain, generates a private encryption key on first launch, and starts the server.
