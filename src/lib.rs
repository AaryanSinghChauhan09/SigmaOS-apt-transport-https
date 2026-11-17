//! SigmaOS-apt-transport-https — HTTPS acquisition method for APT.
//!
//! Inspired by Debian's `apt-transport-https`: the mechanism that lets apt
//! fetch `deb https://...` sources over TLS. This crate models the transport
//! method — parsing `https://` source URIs into (scheme, host, port, path)
//! and reporting the method's capabilities (TLS required, default port 443)
//! — keeping mechanism separate from the actual TLS/network layer.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// The URI was not a valid `https://` source URI.
    #[error("invalid https URI: {0}")]
    InvalidUri(String),
    /// The URI used a non-https scheme.
    #[error("unsupported scheme: {0}")]
    BadScheme(String),
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// A parsed `https://` source URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpsUri {
    /// Hostname (no port, no userinfo).
    pub host: String,
    /// Port (defaults to 443).
    pub port: u16,
    /// Path component (leading `/` kept; empty → `/`).
    pub path: String,
    /// Optional username:password userinfo (percent-encoded as-is).
    pub userinfo: Option<String>,
}

/// The HTTPS transport method descriptor.
#[derive(Debug, Clone, Copy)]
pub struct HttpsMethod;

impl HttpsMethod {
    /// URI scheme this method handles.
    pub const SCHEME: &'static str = "https";
    /// Default port.
    pub const DEFAULT_PORT: u16 = 443;
    /// Whether TLS is mandatory (always true for https).
    pub const REQUIRES_TLS: bool = true;

    /// Whether `uri` uses the `https` scheme.
    pub fn supports(uri: &str) -> bool {
        uri.trim_start().to_ascii_lowercase().starts_with("https://")
    }

    /// Parse an `https://` URI. Rejects other schemes and malformed input.
    pub fn parse(uri: &str) -> ComponentResult<HttpsUri> {
        let uri = uri.trim();
        let rest = uri
            .strip_prefix("https://")
            .ok_or_else(|| {
                if let Some((scheme, _)) = uri.split_once("://") {
                    ComponentError::BadScheme(scheme.to_string())
                } else {
                    ComponentError::InvalidUri(uri.to_string())
                }
            })?;
        if rest.is_empty() {
            return Err(ComponentError::InvalidUri(uri.to_string()));
        }
        // Split authority (up to first '/') from path.
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if authority.is_empty() {
            return Err(ComponentError::InvalidUri(uri.to_string()));
        }
        // Extract optional userinfo.
        let (userinfo, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) => (Some(u.to_string()), h),
            None => (None, authority),
        };
        // Split host:port (handle IPv6 loosely by checking brackets).
        let (host, port) = if hostport.starts_with('[') {
            match hostport.find(']') {
                Some(end) => {
                    let host = &hostport[..=end];
                    let after = &hostport[end + 1..];
                    let port = after.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(443);
                    (host.to_string(), port)
                }
                None => return Err(ComponentError::InvalidUri(uri.to_string())),
            }
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                    (h.to_string(), p.parse().unwrap_or(443))
                }
                _ => (hostport.to_string(), 443),
            }
        };
        if host.is_empty() {
            return Err(ComponentError::InvalidUri(uri.to_string()));
        }
        Ok(HttpsUri { host, port, path: path.to_string(), userinfo })
    }
}

impl HttpsMethod {
    /// Convenience: parse and return the method too.
    pub fn parse_full(uri: &str) -> ComponentResult<(Self, HttpsUri)> {
        Ok((HttpsMethod, Self::parse(uri)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_https() {
        let u = HttpsMethod::parse("https://deb.debian.org/debian").unwrap();
        assert_eq!(u.host, "deb.debian.org");
        assert_eq!(u.port, 443);
        assert_eq!(u.path, "/debian");
        assert!(u.userinfo.is_none());
    }

    #[test]
    fn parses_port_and_userinfo() {
        let u = HttpsMethod::parse("https://user:pass@example.com:8443/pool/main").unwrap();
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, 8443);
        assert_eq!(u.path, "/pool/main");
        assert_eq!(u.userinfo.as_deref(), Some("user:pass"));
    }

    #[test]
    fn root_path_defaults() {
        let u = HttpsMethod::parse("https://example.com").unwrap();
        assert_eq!(u.path, "/");
    }

    #[test]
    fn supports_checks_scheme() {
        assert!(HttpsMethod::supports("https://x/y"));
        assert!(HttpsMethod::supports("  HTTPS://x/y"));
        assert!(!HttpsMethod::supports("http://x/y"));
        assert!(!HttpsMethod::supports("ftp://x/y"));
    }

    #[test]
    fn rejects_non_https() {
        assert!(matches!(HttpsMethod::parse("http://x/y"), Err(ComponentError::BadScheme(_))));
        assert!(matches!(HttpsMethod::parse("not a uri"), Err(ComponentError::InvalidUri(_))));
        assert!(HttpsMethod::parse("https://").is_err());
    }

    #[test]
    fn ipv6_host() {
        let u = HttpsMethod::parse("https://[2001:db8::1]:8443/x").unwrap();
        assert_eq!(u.host, "[2001:db8::1]");
        assert_eq!(u.port, 8443);
    }
}
