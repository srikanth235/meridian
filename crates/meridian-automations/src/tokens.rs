//! Per-run shared-secret tokens that gate the SDK HTTP surface.
//!
//! The runner spawns with the token in its env; the SDK echoes it on every
//! call; the daemon validates against the live in-memory map. Tokens are
//! removed when a run finishes so a leaked token can't be replayed.

use parking_lot::Mutex;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct TokenContext {
    pub automation_id: String,
    pub run_id: i64,
    pub dry_run: bool,
}

#[derive(Clone, Default)]
pub struct TokenStore {
    inner: Arc<Mutex<HashMap<String, TokenContext>>>,
}

impl TokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issue(&self, ctx: TokenContext) -> String {
        let mut buf = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        let token = hex::encode(buf);
        self.inner.lock().insert(token.clone(), ctx);
        token
    }

    pub fn lookup(&self, token: &str) -> Option<TokenContext> {
        self.inner.lock().get(token).cloned()
    }

    pub fn revoke(&self, token: &str) {
        self.inner.lock().remove(token);
    }
}
