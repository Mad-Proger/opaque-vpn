#[cfg(target_os = "windows")]
mod windows_routing;

#[cfg(target_os = "windows")]
pub type RouteManager = windows_routing::DefaultRoute;
