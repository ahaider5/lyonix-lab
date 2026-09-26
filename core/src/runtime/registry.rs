use crate::domain::RuntimeAdapter;
use std::sync::Arc;

#[derive(Default)]
pub struct RuntimeRegistry {
    adapters: Vec<Arc<dyn RuntimeAdapter>>,
}

impl RuntimeRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn register<A: RuntimeAdapter + 'static>(&mut self, adapter: A) { self.adapters.push(Arc::new(adapter)); }
    pub fn get(&self, id: &str) -> Option<Arc<dyn RuntimeAdapter>> { self.adapters.iter().find(|a| a.id() == id).cloned() }
    pub fn ids(&self) -> Vec<String> { self.adapters.iter().map(|a| a.id().to_string()).collect() }
}
