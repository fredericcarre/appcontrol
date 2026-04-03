//! Host Resolution Engine
//!
//! Resolves portable "host" identifiers (FQDN or IP) to agent UUIDs.
//! Used during map import to bind components to agents.
//!
//! Resolution strategies (in order):
//! 1. Exact hostname match
//! 2. FQDN suffix match (host matches start of agent's hostname)
//! 3. IP address match (from agents.ip_addresses JSONB array)

use crate::db::{DbPool, DbUuid};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Resolution status for a single host
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResolutionResult {
    /// Exactly one agent matched
    Resolved {
        agent_id: Uuid,
        agent_hostname: String,
        gateway_id: Option<DbUuid>,
        gateway_name: Option<String>,
        resolved_via: ResolutionMethod,
    },
    /// Multiple agents matched - user must choose
    Multiple { candidates: Vec<AgentCandidate> },
    /// No agents matched - user must select manually
    Unresolved,
}

/// How a host was resolved to an agent
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMethod {
    ExactHostname,
    FqdnSuffix,
    Ip,
    Manual,
    Pattern,
}

impl ResolutionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExactHostname => "exact_hostname",
            Self::FqdnSuffix => "fqdn_suffix",
            Self::Ip => "ip",
            Self::Manual => "manual",
            Self::Pattern => "pattern",
        }
    }
}

impl std::fmt::Display for ResolutionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An agent candidate for resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCandidate {
    pub agent_id: DbUuid,
    pub hostname: String,
    pub gateway_id: Option<DbUuid>,
    pub gateway_name: Option<String>,
    pub ip_addresses: Vec<String>,
    pub matched_via: ResolutionMethod,
}

/// Available agent (for manual selection UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableAgent {
    pub agent_id: DbUuid,
    pub hostname: String,
    pub gateway_id: Option<DbUuid>,
    pub gateway_name: Option<String>,
    pub ip_addresses: Vec<String>,
    pub is_active: bool,
}

use crate::repository::core_queries as repo;

type AgentRow = repo::ResolutionAgentRow;

/// Resolve a host to an agent, scoped to specific gateways.
///
/// Resolution order:
/// 1. Exact hostname match (case-insensitive)
/// 2. FQDN suffix match (host is prefix of agent hostname, followed by '.')
/// 3. IP address match (host is in agents.ip_addresses array)
pub async fn resolve_host_with_options(
    pool: &DbPool,
    host: &str,
    gateway_ids: &[Uuid],
    org_id: DbUuid,
) -> Result<ResolutionResult, sqlx::Error> {
    let host_lower = host.to_lowercase();

    // 1. Try exact hostname match
    let exact_matches: Vec<AgentRow> =
        repo::query_agents_exact_hostname(pool, org_id, &host_lower, gateway_ids).await?;

    if exact_matches.len() == 1 {
        let m = &exact_matches[0];
        return Ok(ResolutionResult::Resolved {
            agent_id: *m.agent_id,
            agent_hostname: m.hostname.clone(),
            gateway_id: m.gateway_id,
            gateway_name: m.gateway_name.clone(),
            resolved_via: ResolutionMethod::ExactHostname,
        });
    }
    if !exact_matches.is_empty() {
        return Ok(ResolutionResult::Multiple {
            candidates: exact_matches
                .into_iter()
                .map(|r| AgentCandidate {
                    agent_id: r.agent_id,
                    hostname: r.hostname,
                    gateway_id: r.gateway_id,
                    gateway_name: r.gateway_name,
                    ip_addresses: r.ip_addresses.0,
                    matched_via: ResolutionMethod::ExactHostname,
                })
                .collect(),
        });
    }

    // 2. Try FQDN suffix match (host is prefix of hostname followed by '.')
    let fqdn_pattern = format!("{}.", host_lower);
    let fqdn_matches: Vec<AgentRow> =
        repo::query_agents_fqdn_match(pool, org_id, &fqdn_pattern, gateway_ids).await?;

    if fqdn_matches.len() == 1 {
        let m = &fqdn_matches[0];
        return Ok(ResolutionResult::Resolved {
            agent_id: *m.agent_id,
            agent_hostname: m.hostname.clone(),
            gateway_id: m.gateway_id,
            gateway_name: m.gateway_name.clone(),
            resolved_via: ResolutionMethod::FqdnSuffix,
        });
    }
    if !fqdn_matches.is_empty() {
        return Ok(ResolutionResult::Multiple {
            candidates: fqdn_matches
                .into_iter()
                .map(|r| AgentCandidate {
                    agent_id: r.agent_id,
                    hostname: r.hostname,
                    gateway_id: r.gateway_id,
                    gateway_name: r.gateway_name,
                    ip_addresses: r.ip_addresses.0,
                    matched_via: ResolutionMethod::FqdnSuffix,
                })
                .collect(),
        });
    }

    // 3. Try IP address match
    let ip_matches: Vec<AgentRow> =
        repo::query_agents_ip_match(pool, org_id, host, gateway_ids).await?;

    if ip_matches.len() == 1 {
        let m = &ip_matches[0];
        return Ok(ResolutionResult::Resolved {
            agent_id: *m.agent_id,
            agent_hostname: m.hostname.clone(),
            gateway_id: m.gateway_id,
            gateway_name: m.gateway_name.clone(),
            resolved_via: ResolutionMethod::Ip,
        });
    }
    if !ip_matches.is_empty() {
        return Ok(ResolutionResult::Multiple {
            candidates: ip_matches
                .into_iter()
                .map(|r| AgentCandidate {
                    agent_id: r.agent_id,
                    hostname: r.hostname,
                    gateway_id: r.gateway_id,
                    gateway_name: r.gateway_name,
                    ip_addresses: r.ip_addresses.0,
                    matched_via: ResolutionMethod::Ip,
                })
                .collect(),
        });
    }

    // No match found
    Ok(ResolutionResult::Unresolved)
}

/// List all available agents on specified gateways (for manual selection)
pub async fn list_available_agents(
    pool: &DbPool,
    gateway_ids: &[Uuid],
    org_id: DbUuid,
) -> Result<Vec<AvailableAgent>, sqlx::Error> {
    let agents: Vec<AgentRow> = repo::query_agents_list(pool, org_id, gateway_ids).await?;

    Ok(agents
        .into_iter()
        .map(|r| AvailableAgent {
            agent_id: r.agent_id,
            hostname: r.hostname,
            gateway_id: r.gateway_id,
            gateway_name: r.gateway_name,
            ip_addresses: r.ip_addresses.0,
            is_active: r.is_active,
        })
        .collect())
}

/// Apply DR pattern rules to suggest a DR hostname from a primary hostname
pub async fn suggest_dr_hostname(
    pool: &DbPool,
    org_id: DbUuid,
    primary_hostname: &str,
) -> Result<Option<String>, sqlx::Error> {
    let rules = repo::fetch_pattern_rules(pool, org_id).await?;

    for rule in rules {
        if let Ok(regex) = regex::Regex::new(&rule.search_pattern) {
            if regex.is_match(primary_hostname) {
                let suggested = regex.replace(primary_hostname, &rule.replace_pattern);
                return Ok(Some(suggested.to_string()));
            }
        }
    }

    Ok(None)
}

/// Try to resolve a suggested DR hostname to an actual agent
pub async fn resolve_dr_agent(
    pool: &DbPool,
    org_id: DbUuid,
    dr_gateway_ids: &[Uuid],
    primary_hostname: &str,
) -> Result<Option<(String, ResolutionResult)>, sqlx::Error> {
    // First, try to suggest a DR hostname using pattern rules
    if let Some(suggested_hostname) = suggest_dr_hostname(pool, org_id, primary_hostname).await? {
        let result =
            resolve_host_with_options(pool, &suggested_hostname, dr_gateway_ids, org_id).await?;
        return Ok(Some((suggested_hostname, result)));
    }

    Ok(None)
}

// Database-specific helper functions moved to repository/core_queries.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_method_display() {
        assert_eq!(
            ResolutionMethod::ExactHostname.to_string(),
            "exact_hostname"
        );
        assert_eq!(ResolutionMethod::FqdnSuffix.to_string(), "fqdn_suffix");
        assert_eq!(ResolutionMethod::Ip.to_string(), "ip");
        assert_eq!(ResolutionMethod::Manual.to_string(), "manual");
        assert_eq!(ResolutionMethod::Pattern.to_string(), "pattern");
    }
}
