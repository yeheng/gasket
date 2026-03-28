//! Search module: re-exports from gasket-history and gasket-semantic

pub use gasket_history::search::*;
pub use gasket_semantic::{
    bytes_to_embedding, cosine_similarity, embedding_to_bytes, top_k_similar, TextEmbedder,
};
