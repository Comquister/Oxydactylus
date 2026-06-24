use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Panel,
    Node,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoleSection {
    #[serde(rename = "type")]
    pub kind: Role,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    pub http_listen:  String,
    pub database_url: String,
    pub jwt_secret:   String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    pub grpc_listen: String,
    pub token:       String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub role:  RoleSection,
    pub panel: Option<PanelConfig>,
    pub node:  Option<NodeConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_panel_role() {
        let raw = r#"
[role]
type = "panel"

[panel]
http_listen  = "0.0.0.0:3000"
database_url = "postgres://localhost/oxy"
jwt_secret   = "test-jwt-secret"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Panel);
        let panel = cfg.panel.unwrap();
        assert_eq!(panel.http_listen, "0.0.0.0:3000");
        assert_eq!(panel.database_url, "postgres://localhost/oxy");
        assert!(cfg.node.is_none());
    }

    #[test]
    fn parses_node_role() {
        let raw = r#"
[role]
type = "node"

[node]
grpc_listen = "0.0.0.0:8080"
token       = "secret-token"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Node);
        let node = cfg.node.unwrap();
        assert_eq!(node.grpc_listen, "0.0.0.0:8080");
        assert_eq!(node.token, "secret-token");
        assert!(cfg.panel.is_none());
    }

    #[test]
    fn parses_both_role() {
        let raw = r#"
[role]
type = "both"

[panel]
http_listen  = "0.0.0.0:3000"
database_url = "postgres://localhost/oxy"
jwt_secret   = "test-jwt-secret"

[node]
grpc_listen = "0.0.0.0:8080"
token       = "secret-token"
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.role.kind, Role::Both);
        assert!(cfg.panel.is_some());
        assert!(cfg.node.is_some());
    }

    #[test]
    fn rejects_unknown_role() {
        let raw = r#"
[role]
type = "invalid"
"#;
        let result: Result<Config, _> = toml::from_str(raw);
        assert!(result.is_err());
    }
}
