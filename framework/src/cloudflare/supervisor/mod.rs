use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use log::info;
use tokio::sync::Mutex;
use utils::context::wait::SuperContext;
use uuid::Uuid;

use crate::cloudflare::supervisor::tunnel::EdgeTunnelClient;

use self::{
    dns::resolve_edge_addr,
    edge::{EdgeTracker, IpPortHost},
    types::{EdgeRegionLocation, Protocol},
};

pub use super::rpc::types::TunnelAuth;

mod datagram;
mod dns;
mod edge;
mod tls;
mod tunnel;

pub mod types;

pub struct Supervisor {
    id: Uuid,
    tracker: EdgeTracker,
    tls: Arc<Mutex<HashMap<Protocol, tls::RootCert>>>,
    auth: TunnelAuth,
}

impl Supervisor {
    pub async fn new(location: &EdgeRegionLocation, auth: TunnelAuth) -> Result<Self> {
        let edges = resolve_edge_addr(location).await?;
        let mut ips = Vec::new();

        for edge in edges {
            for ip in edge.addrs {
                ips.push(IpPortHost {
                    ip,
                    hostname: edge.hostname.clone(),
                    port: edge.port,
                    version: match ip.is_ipv6() {
                        false => edge::IpVersion::Ipv4,
                        true => edge::IpVersion::Ipv6,
                    },
                });
            }
        }

        Ok(Supervisor {
            id: Uuid::new_v4(),
            auth,
            tls: Arc::new(Mutex::new(tls::get_proto_edge_tls_map().await?)),
            tracker: EdgeTracker::new(ips),
        })
    }

    pub async fn start(mut self, ctx: SuperContext) -> Result<()> {
        let tls = self.tls.lock().await.get(&Protocol::QUIC).unwrap().clone(); // todo
        let ip = self.tracker.get(&0).await?;

        let server = EdgeTunnelClient::new(0, self.auth.clone());
        info!("Starting tunnel server");

        let resp = server
            .serve(ctx.clone(), Protocol::QUIC, ip.clone(), tls.clone())
            .await;

        self.tracker.release(&0).await;

        resp
    }
}
