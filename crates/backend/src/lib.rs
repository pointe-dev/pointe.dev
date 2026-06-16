//! backend_lib — re-exports internal modules so integration tests can import them
//! without duplicating the full binary entry-point.
//!
//! Layer: library shim
//! Does NOT cover: the HTTP listener setup in main(), Langfuse prompt fetch,
//!                 fastembed model loading, or database migrations.

pub mod agents;
pub mod capabilities;
pub mod cloudflare;
pub mod config;
pub mod credentials;
pub mod email;
pub mod embeddings;
pub mod handlers;
pub mod langfuse;
pub mod mcp;
pub mod pending;
pub mod pipeline;
pub mod pitch;
pub mod qdrant;
pub mod sessions;
pub mod state;
pub mod stripe;
