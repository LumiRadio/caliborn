#[async_trait::async_trait]
pub trait RoleRepository: Send + Sync + 'static {}
