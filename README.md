# pointe.dev

**AI Product Commercialization & Business Process Automation**

A high-end agency turning bleeding-edge AI engineering into breathtaking user experiences. Built with Rust (Leptos + Axum) for performance, type safety, and elegance.

## 🩰 The Philosophy

Inspired by the pointe dancer metaphor:
- **Behind the Curtain:** Years of grueling practice, complex physics, rigorous engineering
- **To the Audience:** Weightless grace, effortless motion, captivated attention

We hide extreme technical complexity to deliver simple, elegant, effortless experiences.

## 🏗️ Project Structure

```
pointe-dev/
├── crates/
│   ├── frontend/          # Leptos WASM frontend
│   ├── backend/           # Axum server
│   └── shared/            # Shared types & utilities
├── Cargo.toml             # Workspace configuration
└── tailwind.config.js     # Design system tokens
```

## 🚀 Getting Started

### Prerequisites
- Rust 1.70+
- Cargo
- Node.js 18+ (for Tailwind)

### Setup

```bash
# Install Leptos CLI (for frontend builds)
cargo install leptos_cli

# Install Tailwind CSS
npm install
```

### Development

```bash
# Run full stack (frontend + backend)
cargo leptos watch

# Or run separately:
# Backend
cargo run -p backend

# Frontend
cargo leptos build --lib -p frontend
```

## 🎨 Brand Colors

- **Primary Black:** `#0B0B0B` (technical depth, stage background)
- **Pure White:** `#FFFFFF` (clarity, simplicity)
- **Crimson Red:** `#D32F2F` (passion, precision, high performance)

## 📦 Service Offerings

1. **AI Product Commercialization** — Raw models → production SaaS
2. **Business Process Automation** — Spreadsheets & emails → autonomous AI agents
3. **High-Performance Systems** — Rust backends with microsecond latencies

## 📝 License

MIT
