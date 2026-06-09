use crate::error::{OrbitError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    None,
    Open,
    Restricted,
}

const DEFAULT_RESTRICTED_ALLOWED_DOMAINS: &[&str] = &[
    "api.linear.app",
    "api.openai.com",
    "auth.openai.com",
    "chatgpt.com",
    "context7.com",
    "docs.mcp.cloudflare.com",
    "github.com",
    "mcp.cloudflare.com",
    "mcp.mdn.mozilla.net",
    "registry.npmjs.org",
];

pub fn default_allowed_domains() -> Vec<String> {
    normalized_allowed_domains(
        &DEFAULT_RESTRICTED_ALLOWED_DOMAINS
            .iter()
            .map(|domain| (*domain).to_string())
            .collect::<Vec<_>>(),
    )
    .expect("default restricted allowlist must be valid")
}

impl NetworkMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "open" => Ok(Self::Open),
            "restricted" => Ok(Self::Restricted),
            other => Err(OrbitError::Usage(format!("unknown network mode `{other}`"))),
        }
    }
}

pub fn normalize_allowed_domains(allowed_domains: &mut Vec<String>) -> Result<()> {
    *allowed_domains = normalized_allowed_domains(allowed_domains)?;
    Ok(())
}

fn normalized_allowed_domains(allowed_domains: &[String]) -> Result<Vec<String>> {
    if allowed_domains.is_empty() {
        return Err(OrbitError::refused(
            "restricted_network_without_allowlist",
            "restricted network requires at least one --allow-domain entry",
            None,
        ));
    }
    let mut normalized = allowed_domains
        .iter()
        .map(|domain| normalize_allowed_domain(domain))
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    let deduped = normalized.clone();
    normalized.retain(|domain| {
        !deduped
            .iter()
            .any(|candidate| candidate != domain && domain_is_subdomain_of(domain, candidate))
    });
    Ok(normalized)
}

fn domain_is_subdomain_of(domain: &str, candidate_parent: &str) -> bool {
    domain.ends_with(&format!(".{candidate_parent}"))
}

fn normalize_allowed_domain(raw: &str) -> Result<String> {
    let domain = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.contains("..")
        || domain
            .chars()
            .any(|c| matches!(c, '/' | ':' | ',') || c.is_control() || c.is_whitespace())
        || is_ip_literal(&domain)
    {
        return Err(OrbitError::refused(
            "invalid_allow_domain",
            format!("restricted network allow-domain `{raw}` is invalid"),
            None,
        ));
    }
    let labels = domain.split('.').collect::<Vec<_>>();
    if labels.len() < 2 {
        return Err(OrbitError::refused(
            "invalid_allow_domain",
            format!("restricted network allow-domain `{raw}` must be a dotted DNS domain"),
            None,
        ));
    }
    for label in labels {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(OrbitError::refused(
                "invalid_allow_domain",
                format!("restricted network allow-domain `{raw}` is invalid"),
                None,
            ));
        }
    }
    Ok(domain)
}

pub fn domain_allowed(host: &str, allowed_domains: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    allowed_domains.iter().any(|domain| {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        host == domain || host.ends_with(&format!(".{domain}"))
    })
}

pub fn validate_proxy_url(proxy: &str) -> Result<()> {
    if proxy.trim() != proxy
        || proxy.is_empty()
        || proxy.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(invalid_proxy(proxy));
    }
    let Some(scheme_end) = proxy.find("://") else {
        return Err(invalid_proxy(proxy));
    };
    let scheme = proxy[..scheme_end].to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err(invalid_proxy(proxy));
    }
    let rest = &proxy[scheme_end + 3..];
    if rest.is_empty() || rest.contains(['/', '?', '#']) {
        return Err(invalid_proxy(proxy));
    }
    let authority = rest.rsplit('@').next().unwrap_or(rest);
    if authority.is_empty() {
        return Err(invalid_proxy(proxy));
    }
    let (host, port) = split_proxy_host_port(authority).ok_or_else(|| invalid_proxy(proxy))?;
    if !valid_proxy_host(host) {
        return Err(invalid_proxy(proxy));
    }
    if let Some(port) = port {
        let Ok(port) = port.parse::<u16>() else {
            return Err(invalid_proxy(proxy));
        };
        if port == 0 {
            return Err(invalid_proxy(proxy));
        }
    }
    Ok(())
}

fn split_proxy_host_port(authority: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &rest[..end];
        let after = &rest[end + 1..];
        if after.is_empty() {
            return Some((host, None));
        }
        return after.strip_prefix(':').map(|port| (host, Some(port)));
    }
    if authority.matches(':').count() > 1 {
        return None;
    }
    let mut parts = authority.splitn(2, ':');
    let host = parts.next()?;
    Some((host, parts.next()))
}

fn valid_proxy_host(host: &str) -> bool {
    !host.is_empty()
        && (host.parse::<std::net::IpAddr>().is_ok()
            || host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
}

fn invalid_proxy(_proxy: &str) -> OrbitError {
    OrbitError::Usage(
        "proxy must be an http(s) URL with host and optional numeric port, without path, query, fragment, whitespace, or control characters"
            .to_string(),
    )
}

pub fn redact_proxy_url(proxy: &str) -> String {
    let Some(scheme_end) = proxy.find("://") else {
        return "<redacted-proxy>".to_string();
    };
    let scheme = &proxy[..scheme_end];
    let rest = &proxy[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if let Some(at) = rest[..authority_end].rfind('@') {
        format!("{scheme}://<redacted>@{}", &rest[at + 1..])
    } else {
        proxy.to_string()
    }
}

pub fn validate_restricted_command(
    command: &[String],
    allowed_domains: &[String],
) -> Result<Vec<String>> {
    let allowed_domains = normalized_allowed_domains(allowed_domains)?;
    let mut refusals = Vec::new();
    for token in command {
        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "--noproxy" | "--no-proxy" | "--proxy" | "--resolve" | "--connect-to" | "--interface"
        ) || lower.contains("no_proxy=")
        {
            return Err(OrbitError::refused(
                "network_proxy_bypass_option",
                format!("network bypass refused: `{token}` can bypass the restricted proxy plan"),
                None,
            ));
        }

        if let Some(host) = extract_url_host(token) {
            if is_ip_literal(&host) {
                return Err(OrbitError::refused(
                    "network_ip_bypass",
                    format!(
                        "network bypass refused: direct IP `{host}` is not allowlisted by domain"
                    ),
                    None,
                ));
            }
            if !domain_allowed(&host, &allowed_domains) {
                return Err(OrbitError::refused(
                    "network_domain_denied",
                    format!(
                        "network bypass refused: host `{host}` is outside restricted allowlist"
                    ),
                    None,
                ));
            }
        }
    }
    refusals.push("direct IP literals are refused".to_string());
    refusals.push("curl/wget proxy bypass flags are refused where detectable".to_string());
    Ok(refusals)
}

fn extract_url_host(token: &str) -> Option<String> {
    let scheme = token.find("://")?;
    let rest = &token[scheme + 3..];
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host_port = host_port.rsplit('@').next().unwrap_or(host_port);
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or(bracketed)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn is_ip_literal(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_exact_and_subdomain() {
        let allowed = vec!["example.com".to_string()];
        assert!(domain_allowed("example.com", &allowed));
        assert!(domain_allowed("api.example.com", &allowed));
        assert!(!domain_allowed("evil-example.com", &allowed));
    }

    #[test]
    fn validates_proxy_urls_before_proxy_config_generation() {
        assert!(validate_proxy_url("http://proxy.example:8080").is_ok());
        assert!(validate_proxy_url("https://user:pass@proxy.example:8443").is_ok());
        assert!(validate_proxy_url("http://[::1]:8080").is_ok());
        assert!(validate_proxy_url("socks5://proxy.example:1080").is_err());
        assert!(validate_proxy_url("http://proxy.example/path").is_err());
        assert!(validate_proxy_url("http://proxy.example:bad").is_err());
        assert!(
            validate_proxy_url("http://proxy.example:8080\nacl evil dstdomain .evil.com").is_err()
        );
    }

    #[test]
    fn redacts_proxy_credentials() {
        assert_eq!(
            redact_proxy_url("http://user:pass@proxy.example:8080"),
            "http://<redacted>@proxy.example:8080"
        );
        assert_eq!(
            redact_proxy_url("http://proxy.example:8080"),
            "http://proxy.example:8080"
        );
        assert_eq!(
            redact_proxy_url("http://proxy.example/path@value"),
            "http://proxy.example/path@value"
        );
        assert_eq!(
            redact_proxy_url("http://proxy.example?token@value"),
            "http://proxy.example?token@value"
        );
        assert_eq!(
            redact_proxy_url("http://proxy.example#token@value"),
            "http://proxy.example#token@value"
        );
        assert_eq!(
            redact_proxy_url("http://alice:token@secret@proxy.example:8080"),
            "http://<redacted>@proxy.example:8080"
        );
    }

    #[test]
    fn extracts_hosts_with_userinfo() {
        assert_eq!(
            extract_url_host("https://user:pass@example.com:8443/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_url_host("https://user:pass@[::1]:8443/path"),
            Some("::1".to_string())
        );
    }

    #[test]
    fn restricted_refuses_empty_allowlist_and_ip_literals() {
        let command = vec!["curl".to_string(), "https://example.com".to_string()];
        assert!(validate_restricted_command(&command, &[]).is_err());
        let invalid_empty = vec![" ".to_string()];
        assert!(validate_restricted_command(&command, &invalid_empty).is_err());
        let invalid_ip_domain = vec!["1.2.3.4".to_string()];
        assert!(validate_restricted_command(&command, &invalid_ip_domain).is_err());
        let invalid_single_label = vec!["internal".to_string()];
        assert!(validate_restricted_command(&command, &invalid_single_label).is_err());
        let mut mixed_case = vec![" Example.COM. ".to_string(), "example.com".to_string()];
        normalize_allowed_domains(&mut mixed_case).unwrap();
        assert_eq!(mixed_case, vec!["example.com".to_string()]);
        let mut overlapping = vec![
            "api.example.com".to_string(),
            "example.com".to_string(),
            "docs.mcp.cloudflare.com".to_string(),
            "mcp.cloudflare.com".to_string(),
        ];
        normalize_allowed_domains(&mut overlapping).unwrap();
        assert_eq!(
            overlapping,
            vec!["example.com".to_string(), "mcp.cloudflare.com".to_string()]
        );
        assert_eq!(
            default_allowed_domains(),
            vec![
                "api.linear.app".to_string(),
                "api.openai.com".to_string(),
                "auth.openai.com".to_string(),
                "chatgpt.com".to_string(),
                "context7.com".to_string(),
                "github.com".to_string(),
                "mcp.cloudflare.com".to_string(),
                "mcp.mdn.mozilla.net".to_string(),
                "registry.npmjs.org".to_string(),
            ]
        );

        let allowed = vec!["example.com".to_string()];
        let ip = vec!["curl".to_string(), "https://1.2.3.4".to_string()];
        assert!(validate_restricted_command(&ip, &allowed).is_err());
        let ipv6 = vec!["curl".to_string(), "https://[::1]/".to_string()];
        assert!(validate_restricted_command(&ipv6, &allowed).is_err());
        let userinfo = vec![
            "curl".to_string(),
            "https://user:pass@example.com/path".to_string(),
        ];
        assert!(validate_restricted_command(&userinfo, &allowed).is_ok());
        let masked_host = vec![
            "curl".to_string(),
            "https://example.com@evil.test/path".to_string(),
        ];
        let masked_err = validate_restricted_command(&masked_host, &allowed).unwrap_err();
        assert!(matches!(
            masked_err,
            OrbitError::Refused {
                code: "network_domain_denied",
                ..
            }
        ));
        let userinfo_ipv6 = vec![
            "curl".to_string(),
            "https://user:pass@[::1]:8443/".to_string(),
        ];
        let ipv6_err = validate_restricted_command(&userinfo_ipv6, &allowed).unwrap_err();
        assert!(matches!(
            ipv6_err,
            OrbitError::Refused {
                code: "network_ip_bypass",
                ..
            }
        ));
    }

    #[test]
    fn restricted_refuses_denied_url_and_proxy_bypass() {
        let allowed = vec!["example.com".to_string()];
        let denied = vec!["curl".to_string(), "https://evil.test".to_string()];
        assert!(validate_restricted_command(&denied, &allowed).is_err());
        let bypass = vec![
            "curl".to_string(),
            "--noproxy".to_string(),
            "*".to_string(),
            "https://example.com".to_string(),
        ];
        assert!(validate_restricted_command(&bypass, &allowed).is_err());
    }
}
