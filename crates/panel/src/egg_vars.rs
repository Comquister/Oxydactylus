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

fn parse_rules(rules: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = rules;
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix("regex:/") {
            if let Some(end) = rest.find('/') {
                result.push(format!("regex:/{}/", &rest[..end]));
                remaining = &rest[end + 1..];
                remaining = remaining.strip_prefix('|').unwrap_or(remaining);
            } else {
                result.push(remaining.to_string());
                break;
            }
        } else if let Some(pos) = remaining.find('|') {
            result.push(remaining[..pos].to_string());
            remaining = &remaining[pos + 1..];
        } else {
            result.push(remaining.to_string());
            break;
        }
    }
    result
}

pub fn validate_var(env_variable: &str, value: &str, rules: &str) -> crate::error::Result<()> {
    let parts: Vec<String> = parse_rules(rules);
    let nullable = parts.iter().any(|r| r == "nullable");

    if value.is_empty() {
        if nullable {
            return Ok(());
        }
        if parts.iter().any(|r| r == "required") {
            return Err(crate::error::PanelError::Validation(
                format!("{}: value is required", env_variable)
            ));
        }
        return Ok(());
    }

    let is_integer_field = parts.iter().any(|r| r == "integer");

    for rule in &parts {
        if rule == "required" || rule == "nullable" || rule == "string" {
            continue;
        }
        if rule == "integer" {
            value.parse::<i64>().map_err(|_| crate::error::PanelError::Validation(
                format!("{}: must be an integer", env_variable)
            ))?;
        } else if rule == "boolean" {
            if value != "true" && value != "false" {
                return Err(crate::error::PanelError::Validation(
                    format!("{}: must be true or false", env_variable)
                ));
            }
        } else if let Some(n) = rule.strip_prefix("max:") {
            if is_integer_field {
                let max_val: i64 = n.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: invalid max rule", env_variable)
                ))?;
                let int_val: i64 = value.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: must be an integer", env_variable)
                ))?;
                if int_val > max_val {
                    return Err(crate::error::PanelError::Validation(
                        format!("{}: must be <= {}", env_variable, max_val)
                    ));
                }
            } else {
                let max: usize = n.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: invalid max rule", env_variable)
                ))?;
                if value.len() > max {
                    return Err(crate::error::PanelError::Validation(
                        format!("{}: exceeds max length {}", env_variable, max)
                    ));
                }
            }
        } else if let Some(n) = rule.strip_prefix("min:") {
            if is_integer_field {
                let min_val: i64 = n.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: invalid min rule", env_variable)
                ))?;
                let int_val: i64 = value.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: must be an integer", env_variable)
                ))?;
                if int_val < min_val {
                    return Err(crate::error::PanelError::Validation(
                        format!("{}: must be >= {}", env_variable, min_val)
                    ));
                }
            } else {
                let min: usize = n.parse().map_err(|_| crate::error::PanelError::Validation(
                    format!("{}: invalid min rule", env_variable)
                ))?;
                if value.len() < min {
                    return Err(crate::error::PanelError::Validation(
                        format!("{}: below min length {}", env_variable, min)
                    ));
                }
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
    let rows: Vec<(String, Option<String>, Option<String>, bool)> = sqlx::query_as(
        "SELECT env_variable, default_val, rules, user_editable FROM egg_variables WHERE egg_id = $1",
    )
    .bind(egg_id)
    .fetch_all(pool)
    .await?;

    let mut resolved = Vec::new();
    for (env_var, default, rules, user_editable) in rows {
        let val = if user_editable {
            user_vars.get(&env_var).cloned().or(default).unwrap_or_default()
        } else {
            default.unwrap_or_default()
        };
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

    #[test]
    fn regex_with_alternation_in_pattern() {
        // regex:/^(a|b)$/ must not be split on the | inside the pattern
        assert!(validate_var("X", "a", r"required|regex:/^(a|b)$/").is_ok());
        assert!(validate_var("X", "b", r"required|regex:/^(a|b)$/").is_ok());
        assert!(validate_var("X", "c", r"required|regex:/^(a|b)$/").is_err());
    }

    #[test]
    fn integer_max_is_numeric_not_length() {
        // "1024" has length 4, but value 1024; max:500 should reject it numerically
        assert!(validate_var("MEM", "1024", "required|integer|max:500").is_err());
    }

    #[test]
    fn integer_min_is_numeric_not_length() {
        // "5" has length 1; min:3 should accept it numerically (5 >= 3)
        assert!(validate_var("MEM", "5", "required|integer|min:3").is_ok());
    }
}
