pub mod error;
pub mod mcp;
pub mod middleware;
pub mod routes;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = wickeban_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;
