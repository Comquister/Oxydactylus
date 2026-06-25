use std::collections::HashMap;
use regex::Regex;
use sqlx::PgPool;
use uuid::Uuid;
use crate::error::Result;

pub fn resolve_vars(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, val) in vars {
        result = result.replace(&format!("{{{{{}}}}}", key), val);
    }
    result
}

pub fn validate_var(env_variable: &str, value: &str, rules: &str) -> crate::error::Result<()> {
    let parts: Vec<&str> = rules.split('|').collect();
    let nullable = parts.contains(&"nullable");

    if value.is_empty() {
        if nullable {
            return Ok(());
        }
        if parts.contains(&"required") {
            return Err(crate::error::PanelError::Validation(
                format!("{}: value is required", env_variable)
            ));
        }
        return Ok(());
    }

    for rule in &parts {
        if *rule == "required" || *rule == "nullable" || *rule == "string" {
            continue;
        }
        if *rule == "integer" {
            value.parse::<i64>().map_err(|_| crate::error::PanelError::Validation(
                format!("{}: must be an integer", env_variable)
            ))?;
        } else if *rule == "boolean" {
            if value != "true" && value != "false" {
                return Err(crate::error::PanelError::Validation(
                    format!("{}: must be true or false", env_variable)
                ));
            }
        } else if let Some(n) = rule.strip_prefix("max:") {
            let max: usize = n.parse().map_err(|_| crate::error::PanelError::Validation(
                format!("{}: invalid max rule", env_variable)
            ))?;
            if value.len() > max {
                return Err(crate::error::PanelError::Validation(
                    format!("{}: exceeds max length {}", env_variable, max)
                ));
            }
        } else if let Some(n) = rule.strip_prefix("min:") {
            let min: usize = n.parse().map_err(|_| crate::error::PanelError::Validation(
                format!("{}: invalid min rule", env_variable)
            ))?;
            if value.len() < min {
                return Err(crate::error::PanelError::Validation(
                    format!("{}: below min length {}", env_variable, min)
                ));
            }
        } else if let Some(pat) = rule.strip_prefix("regex:/").and_then(|s| s.strip_suffix('/')) {
            let re = Regex::new(pat).map_err(|_| crate::error::PanelError::Validation(
                format!("{}: invalid regex pattern", env_variable)
            ))?;
            if !re.is_match(value) {
                return Err(crate::error::PanelError::Validation(
                    format!("{}: does not match required pattern", env_variable)
                ));
            }
        }
    }
    Ok(())
}

pub async fn load_egg_env(
    pool: &PgPool,
    egg_id: Uuid,
    user_vars: HashMap<String, String>,
) -> Result<Vec<String>> {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT env_variable, default_val, rules FROM egg_variables WHERE egg_id = $1",
    )
    .bind(egg_id)
    .fetch_all(pool)
    .await?;

    let mut resolved = Vec::new();
    for (env_var, default, rules) in rows {
        let val = user_vars
            .get(&env_var)
            .cloned()
            .or(default)
            .unwrap_or_default();
        if let Some(r) = &rules {
            validate_var(&env_var, &val, r)?;
        }
        resolved.push(format!("{}={}", env_var, val));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn resolve_single_var() {
        let mut vars = HashMap::new();
        vars.insert("NAME".to_string(), "world".to_string());
        assert_eq!(resolve_vars("Hello {{NAME}}!", &vars), "Hello world!");
    }

    #[test]
    fn resolve_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("A".to_string(), "foo".to_string());
        vars.insert("B".to_string(), "bar".to_string());
        assert_eq!(resolve_vars("{{A}}-{{B}}", &vars), "foo-bar");
    }

    #[test]
    fn resolve_unknown_var_left_as_is() {
        let vars = HashMap::new();
        assert_eq!(resolve_vars("{{UNKNOWN}}", &vars), "{{UNKNOWN}}");
    }

    #[test]
    fn resolve_no_vars() {
        let vars = HashMap::new();
        assert_eq!(resolve_vars("no placeholders", &vars), "no placeholders");
    }

    #[test]
    fn required_rejects_empty() {
        assert!(validate_var("PORT", "", "required").is_err());
    }

    #[test]
    fn required_accepts_nonempty() {
        assert!(validate_var("PORT", "25565", "required").is_ok());
    }

    #[test]
    fn nullable_accepts_empty() {
        assert!(validate_var("OPT", "", "nullable").is_ok());
    }

    #[test]
    fn integer_rejects_text() {
        assert!(validate_var("PORT", "abc", "required|integer").is_err());
    }

    #[test]
    fn integer_accepts_number() {
        assert!(validate_var("PORT", "25565", "required|integer").is_ok());
    }

    #[test]
    fn max_rejects_too_long() {
        assert!(validate_var("X", "hello", "required|max:3").is_err());
    }

    #[test]
    fn max_accepts_within_limit() {
        assert!(validate_var("X", "hi", "required|max:3").is_ok());
    }

    #[test]
    fn min_rejects_too_short() {
        assert!(validate_var("X", "a", "required|min:3").is_err());
    }

    #[test]
    fn regex_accepts_match() {
        assert!(validate_var("F", "server.jar", r"required|regex:/^[\w\d._-]+\.jar$/").is_ok());
    }

    #[test]
    fn regex_rejects_no_match() {
        assert!(validate_var("F", "server.zip", r"required|regex:/^[\w\d._-]+\.jar$/").is_err());
    }
}
