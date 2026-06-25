use std::collections::HashMap;
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

pub async fn load_egg_env(
    pool: &PgPool,
    egg_id: Uuid,
    user_vars: HashMap<String, String>,
) -> Result<Vec<String>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT env_variable, default_val FROM egg_variables WHERE egg_id = $1",
    )
    .bind(egg_id)
    .fetch_all(pool)
    .await?;

    let mut resolved = Vec::new();
    for (env_var, default) in rows {
        let val = user_vars
            .get(&env_var)
            .cloned()
            .or(default)
            .unwrap_or_default();
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
}
