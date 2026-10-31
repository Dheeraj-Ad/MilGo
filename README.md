# MilGo

MilGo is a game-inspired productivity platform that turns routine tasks into engaging, rewarding experiences. The project blends productivity, gamification, and modern full-stack engineering to create a more immersive way to manage work and personal goals.

The concept is simple: instead of treating tasks as plain checklists, users interact with a playful system where progress feels like a journey, not just a chore.

## Overview

MilGo is built to explore:
- gamified task experiences
- interactive user engagement
- modern frontend architecture
- performant backend services
- a strong product-first engineering approach

This project currently includes the Taskocalypse experience, a 3D, game-like environment where chores become quests and progress is represented through a dynamic, interactive world.

## Tech Stack

- Frontend: React, TypeScript, Vite, Three.js
- Backend: Rust, Axum
- Database: SQLite
- Language mix: TypeScript, Rust, JavaScript

## Project Structure

```bash
MilGo/
├── taskocalypse/
│   ├── public/
│   ├── server/
│   ├── src/
│   ├── package.json
│   ├── vite.config.ts
│   └── README.md
└── README.md
```

## Features

- Game-inspired productivity experience
- Task-based progression and engagement model
- 3D interactive environment
- Modern TypeScript frontend
- Rust-based backend services
- SQLite storage for local data handling
- Easy local setup for development

## Getting Started

### Prerequisites

- Node.js 18+
- Rust toolchain
- npm or yarn
- SQLite support

### Install frontend dependencies

```bash
cd taskocalypse
npm install
```

### Build the frontend

```bash
npm run build
```

### Start the backend

```bash
cargo run --manifest-path server/Cargo.toml
```

### Run locally

Open:

```bash
http://localhost:8080
```

## Development Notes

The project includes setup guidance for running the application locally and configuring environment variables where required.

## Why MilGo?

Because productivity tools should not feel mechanical. MilGo aims to bring joy, progression, and motivation into everyday task management by making digital work feel more interactive and rewarding.

## License

This project is currently distributed under its repository's applicable license terms. Please check the repository for the exact licensing details.

## Repository

GitHub: https://github.com/Dheeraj-Ad/MilGo

## Status

This project is actively evolving as a creative full-stack experimentation in gamified productivity.
