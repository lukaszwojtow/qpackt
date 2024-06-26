use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_files::Files;
use actix_web::{App, HttpServer};
use actix_web::dev::{Server, ServerHandle};
use log::{debug, error, info, warn};
use rand::{Rng, thread_rng};
use tokio::sync::RwLock;
use url::Url;

use crate::constants::VERSIONS_SUBDIRECTORY;
use crate::dao::version::{Version, VersionName};
use crate::error::{QpacktError, Result};
use crate::manager::strategy::Strategy;

/// All servers for versions will be started on localhost:port where starting port is the value below.
const START_PORT: u16 = 10_000;

/// Contains details for various versions' servers.  
pub(crate) struct Versions {
    versions: RwLock<Vec<VersionServer>>,
}

pub(crate) struct VersionServer {
    pub(crate) version: Version,
    port: u16,
    upstream: Arc<Url>,
    server_handle: Option<ServerHandle>,
}

impl Versions {
    pub(super) async fn start(versions: Vec<Version>, run_dir: &Path) -> Self {
        let mut versions = build_version_servers(versions);
        start_version_servers(&mut versions, run_dir).await;
        Self { versions: RwLock::new(versions) }
    }

    /// Tries to pick a new [Url] and [VersionName] for request based on [Strategy] and request query.
    /// First try url param matching,
    /// then calculate total weights and pick some version proportionally.
    pub(super) async fn pick_upstream(&self, query: &str) -> Result<(Arc<Url>, VersionName)> {
        let versions = self.versions.read().await;
        // Try UrlParam matching first.
        for v in versions.iter() {
            if let Strategy::UrlParam(needle) = &v.version.strategy {
                if query.contains(needle) {
                    debug!("Picking version {} by UrlParam (needle: {}, query: {})", v.version.name, needle, query);
                    return Ok((v.upstream.clone(), v.version.name.clone()));
                }
            }
        }
        // Add up all weights.
        let sum_weights = versions.iter().map(|v| if let Strategy::Weight(w) = v.version.strategy { w as i32 } else { 0 }).sum::<i32>();
        // Pick some version proportionally.
        let mut cut = thread_rng().gen_range(1..=sum_weights);
        for v in versions.iter() {
            if let Strategy::Weight(w) = v.version.strategy {
                cut -= w as i32;
                if cut <= 0 {
                    debug!("Picking version {} by Weight", v.version.name);
                    return Ok((v.upstream.clone(), v.version.name.clone()));
                }
            }
        }
        error!("Unable to find working version");
        Err(QpacktError::ProxyError)
    }

    /// Loops over current versions and updates their strategies with new ones.
    /// Will not create/delete a [Version].
    pub(super) async fn update_strategies(&self, new: &[Version]) {
        let mut versions = self.versions.write().await;
        for current_version in versions.iter_mut() {
            for updated in new {
                if updated.name == current_version.version.name {
                    current_version.version.strategy = updated.strategy.clone();
                    break;
                }
            }
        }
    }

    /// Deletes [Version] and stops its task.
    pub(super) async fn delete_version(&self, name: &VersionName) {
        let mut versions = self.versions.write().await;
        for (i, v) in versions.iter().enumerate() {
            if &v.version.name == name {
                let version = versions.remove(i);
                if let Some(server_handle) = version.server_handle {
                    server_handle.stop(true).await;
                } else {
                    warn!("No running task for version `{}` that's being removed", name);
                }
                break;
            }
        }
    }

    /// Adds [Version] and starts [VersionServer] for it.
    pub(super) async fn add_version(&self, version: Version, run_dir: &Path) {
        let mut versions = self.versions.write().await;
        let next_port = versions.iter().map(|v| v.port).max().unwrap_or(START_PORT) + 1;
        let mut server = build_version_server(version, next_port);
        start_version_server(&mut server, run_dir).await;
        versions.push(server);
    }

    /// Gets [Url] for cookie.
    pub(super) async fn get_url_for_cookie(&self, cookie: &str) -> Option<(Arc<Url>, VersionName)> {
        let versions = self.versions.read().await;
        versions.iter().find(|v| v.version.name.matches(cookie)).map(|found| (found.upstream.clone(), found.version.name.clone()))
    }
}

/// Starts a normal 'Files' server to serve a version.
async fn start_version_servers(handlers: &mut Vec<VersionServer>, run_dir: &Path) {
    for handler in handlers {
        info!("Starting version {} on port {}", handler.version.name, handler.port);
        start_version_server(handler, run_dir).await;
    }
}

async fn start_version_server(version_handler: &mut VersionServer, run_dir: &Path) {
    let path = run_dir.join(VERSIONS_SUBDIRECTORY).join(&version_handler.version.web_root);
    let port = version_handler.port;
    let server = build_server(path, port);
    let handle = server.handle();
    tokio::spawn(server);
    version_handler.server_handle = Some(handle);
}

fn build_server(path: PathBuf, port: u16) -> Server {
    HttpServer::new(move || App::new().service(Files::new("/", path.clone()).index_file("index.html")))
        .bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
        .unwrap()
        .run()
}

fn build_version_servers(versions: Vec<Version>) -> Vec<VersionServer> {
    let mut handlers = Vec::with_capacity(versions.len());
    for (id, version) in versions.into_iter().enumerate() {
        let port = START_PORT + id as u16;
        handlers.push(build_version_server(version, port))
    }
    handlers
}

fn build_version_server(version: Version, port: u16) -> VersionServer {
    VersionServer {
        port,
        upstream: Arc::new(Url::parse(format!("http://localhost:{}", port).as_str()).unwrap()),
        version,
        server_handle: None,
    }
}
