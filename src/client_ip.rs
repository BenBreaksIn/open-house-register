use crate::{ApiError, AppState};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, StatusCode, request::Parts},
};
use ipnet::IpNet;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

#[derive(Clone, Debug)]
pub enum ClientIpSource {
    Direct,
    Vercel,
    Render,
    TrustedProxies(Vec<IpNet>),
}

impl ClientIpSource {
    pub fn from_env() -> Result<Self, String> {
        if let Ok(value) = std::env::var("TRUSTED_PROXY_CIDRS")
            && !value.trim().is_empty()
        {
            let nets: Vec<IpNet> = value
                .split(',')
                .map(|v| {
                    v.trim()
                        .parse()
                        .map_err(|_| "TRUSTED_PROXY_CIDRS must be comma-separated IP networks.")
                })
                .collect::<Result<_, _>>()?;
            if nets.iter().any(|n| n.prefix_len() == 0) {
                return Err("Do not trust every address as a proxy.".into());
            }
            return Ok(Self::TrustedProxies(nets));
        }
        if std::env::var("VERCEL").as_deref() == Ok("1") {
            return Ok(Self::Vercel);
        }
        if std::env::var("RENDER").as_deref() == Ok("true") {
            return Ok(Self::Render);
        }
        Ok(Self::Direct)
    }

    pub fn resolve(&self, headers: &HeaderMap, peer: Option<IpAddr>) -> Result<IpAddr, ApiError> {
        let invalid = || {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "Unable to identify this connection. Please contact the host.",
            )
        };
        let single = |name: &str| -> Result<IpAddr, ApiError> {
            if headers.get_all(name).iter().count() != 1 {
                return Err(invalid());
            }
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok())
                .ok_or_else(invalid)
        };
        let ip = match self {
            Self::Direct => peer.ok_or_else(invalid)?,
            // These modes are selected by server environment, never request headers.
            // Provider ingress must remain the only public path to the origin.
            Self::Vercel => single("x-vercel-forwarded-for")?,
            Self::Render => single("cf-connecting-ip")?,
            Self::TrustedProxies(nets) => {
                let mut current = peer.ok_or_else(invalid)?.to_canonical();
                if nets.iter().any(|n| n.contains(&current)) {
                    if headers.get_all("x-forwarded-for").iter().count() != 1 {
                        return Err(invalid());
                    }
                    let chain = headers
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(invalid)?;
                    if chain.len() > 1024 {
                        return Err(invalid());
                    }
                    for part in chain.rsplit(',') {
                        if !nets.iter().any(|n| n.contains(&current)) {
                            break;
                        }
                        current = part
                            .trim()
                            .parse::<IpAddr>()
                            .map_err(|_| invalid())?
                            .to_canonical();
                    }
                }
                current
            }
        };
        Ok(ip.to_canonical())
    }
}

pub struct ClientKey(pub String);
impl FromRequestParts<AppState> for ClientKey {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        let ip = state.config.client_ip.resolve(&parts.headers, peer)?;
        // Group IPv6 by /64 so rotating interface addresses does not reset a budget.
        let source = match ip {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("{}/64", Ipv6Addr::from(u128::from(ip) & (u128::MAX << 64))),
        };
        Ok(Self(
            state.config.keyed_digest("rate-limit-source", &source),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(name: &str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
        h
    }
    #[test]
    fn forwarding_requires_a_trusted_ingress() {
        let peer = "192.0.2.1".parse().unwrap();
        let h = headers("x-forwarded-for", "198.51.100.1");
        assert_eq!(
            ClientIpSource::Direct.resolve(&h, Some(peer)).unwrap(),
            peer
        );
        let proxies = ClientIpSource::TrustedProxies(vec!["10.0.0.0/24".parse().unwrap()]);
        assert_eq!(proxies.resolve(&h, Some(peer)).unwrap(), peer);
        let h = headers("x-forwarded-for", "203.0.113.99, 198.51.100.2, 10.0.0.2");
        assert_eq!(
            proxies
                .resolve(&h, Some("10.0.0.1".parse().unwrap()))
                .unwrap(),
            "198.51.100.2".parse::<IpAddr>().unwrap()
        );
        assert!(
            proxies
                .resolve(&HeaderMap::new(), Some("10.0.0.1".parse().unwrap()))
                .is_err()
        );
        assert!(ClientIpSource::Direct.resolve(&h, None).is_err());
    }
    #[test]
    fn provider_headers_are_single_valid_addresses() {
        let mut h = headers("x-vercel-forwarded-for", "::ffff:192.0.2.1");
        h.insert("x-forwarded-for", "198.51.100.7".parse().unwrap());
        assert_eq!(
            ClientIpSource::Vercel.resolve(&h, None).unwrap(),
            "192.0.2.1".parse::<IpAddr>().unwrap()
        );
        assert!(ClientIpSource::Render.resolve(&h, None).is_err());
        h.insert("cf-connecting-ip", "2001:db8::1".parse().unwrap());
        assert_eq!(
            ClientIpSource::Render.resolve(&h, None).unwrap(),
            "2001:db8::1".parse::<IpAddr>().unwrap()
        );
        h.append("x-vercel-forwarded-for", "192.0.2.2".parse().unwrap());
        assert!(ClientIpSource::Vercel.resolve(&h, None).is_err());
        for value in ["", "not-an-ip", "192.0.2.1, 192.0.2.2"] {
            assert!(
                ClientIpSource::Vercel
                    .resolve(&headers("x-vercel-forwarded-for", value), None)
                    .is_err()
            );
        }
    }
}
