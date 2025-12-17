// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Configuration value types that support static values or references.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A configuration value that can be static or a reference to be resolved
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    /// A reference to a secret (always resolves to string, then parsed to T)
    Secret {
        name: String,
    },
    
    /// A reference to an environment variable
    EnvironmentVariable {
        name: String,
        default: Option<String>,
    },
    
    /// A static value of type T
    Static(T),
}

// Type aliases for common cases
pub type ConfigValueString = ConfigValue<String>;
pub type ConfigValueU16 = ConfigValue<u16>;
pub type ConfigValueU64 = ConfigValue<u64>;
pub type ConfigValueBool = ConfigValue<bool>;

// Custom serialization to support the discriminated union format
impl<T> Serialize for ConfigValue<T>
where
    T: Serialize + DeserializeOwned + Clone,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        
        match self {
            ConfigValue::Secret { name } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "Secret")?;
                map.serialize_entry("name", name)?;
                map.end()
            }
            ConfigValue::EnvironmentVariable { name, default } => {
                let size = if default.is_some() { 3 } else { 2 };
                let mut map = serializer.serialize_map(Some(size))?;
                map.serialize_entry("kind", "EnvironmentVariable")?;
                map.serialize_entry("name", name)?;
                if let Some(d) = default {
                    map.serialize_entry("default", d)?;
                }
                map.end()
            }
            ConfigValue::Static(value) => value.serialize(serializer),
        }
    }
}

// Custom deserialization to support POSIX format, structured format, and static values
impl<'de, T> Deserialize<'de> for ConfigValue<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        use serde_json::Value;
        
        let value = Value::deserialize(deserializer)?;
        
        // Try to deserialize as structured format with "kind" field
        if let Value::Object(ref map) = value {
            if let Some(Value::String(kind)) = map.get("kind") {
                match kind.as_str() {
                    "Secret" => {
                        let name = map.get("name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| D::Error::missing_field("name"))?
                            .to_string();
                        
                        return Ok(ConfigValue::Secret { name });
                    }
                    "EnvironmentVariable" => {
                        let name = map.get("name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| D::Error::missing_field("name"))?
                            .to_string();
                        
                        let default = map.get("default")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        
                        return Ok(ConfigValue::EnvironmentVariable { name, default });
                    }
                    _ => {
                        return Err(D::Error::custom(format!("Unknown kind: {}", kind)));
                    }
                }
            }
        }
        
        // Try to parse POSIX format for any type (the string will be parsed to T later)
        if let Value::String(s) = &value {
            if let Some(env_ref) = parse_posix_env_var(s) {
                return Ok(env_ref);
            }
        }
        
        // Otherwise, deserialize as static value
        let static_value: T = serde_json::from_value(value)
            .map_err(|e| D::Error::custom(format!("Failed to deserialize as static value: {}", e)))?;
        
        Ok(ConfigValue::Static(static_value))
    }
}

/// Parse POSIX-style environment variable reference like ${VAR:-default} or ${VAR}
fn parse_posix_env_var<T>(s: &str) -> Option<ConfigValue<T>>
where
    T: Clone + Serialize + DeserializeOwned,
{
    // Check if it matches ${...} pattern
    if !s.starts_with("${") || !s.ends_with('}') {
        return None;
    }
    
    let inner = &s[2..s.len()-1];
    
    // Check for default value syntax: VAR:-default
    if let Some(colon_pos) = inner.find(":-") {
        let name = inner[..colon_pos].to_string();
        let default = Some(inner[colon_pos+2..].to_string());
        Some(ConfigValue::EnvironmentVariable { name, default })
    } else {
        // No default value
        let name = inner.to_string();
        Some(ConfigValue::EnvironmentVariable { name, default: None })
    }
}

// Implement Default for ConfigValue when T implements Default
impl<T> Default for ConfigValue<T>
where
    T: Serialize + DeserializeOwned + Clone + Default,
{
    fn default() -> Self {
        ConfigValue::Static(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    
    #[test]
    fn test_deserialize_static_string() {
        let json = r#""hello""#;
        let value: ConfigValue<String> = serde_json::from_str(json).unwrap();
        assert_eq!(value, ConfigValue::Static("hello".to_string()));
    }
    
    #[test]
    fn test_deserialize_static_number() {
        let json = r#"5432"#;
        let value: ConfigValue<u16> = serde_json::from_str(json).unwrap();
        assert_eq!(value, ConfigValue::Static(5432));
    }
    
    #[test]
    fn test_deserialize_posix_with_default() {
        let json = r#""${DB_PORT:-5432}""#;
        let value: ConfigValue<String> = serde_json::from_str(json).unwrap();
        match value {
            ConfigValue::EnvironmentVariable { name, default } => {
                assert_eq!(name, "DB_PORT");
                assert_eq!(default, Some("5432".to_string()));
            }
            _ => panic!("Expected EnvironmentVariable variant"),
        }
    }
    
    #[test]
    fn test_deserialize_posix_without_default() {
        let json = r#""${MY_VAR}""#;
        let value: ConfigValue<String> = serde_json::from_str(json).unwrap();
        match value {
            ConfigValue::EnvironmentVariable { name, default } => {
                assert_eq!(name, "MY_VAR");
                assert_eq!(default, None);
            }
            _ => panic!("Expected EnvironmentVariable variant"),
        }
    }
    
    #[test]
    fn test_deserialize_structured_env_var() {
        let json = r#"{"kind": "EnvironmentVariable", "name": "DB_PASSWORD", "default": "secret"}"#;
        let value: ConfigValue<String> = serde_json::from_str(json).unwrap();
        match value {
            ConfigValue::EnvironmentVariable { name, default } => {
                assert_eq!(name, "DB_PASSWORD");
                assert_eq!(default, Some("secret".to_string()));
            }
            _ => panic!("Expected EnvironmentVariable variant"),
        }
    }
    
    #[test]
    fn test_deserialize_structured_secret() {
        let json = r#"{"kind": "Secret", "name": "my-secret"}"#;
        let value: ConfigValue<String> = serde_json::from_str(json).unwrap();
        match value {
            ConfigValue::Secret { name } => {
                assert_eq!(name, "my-secret");
            }
            _ => panic!("Expected Secret variant"),
        }
    }
    
    #[test]
    fn test_serialize_static() {
        let value = ConfigValue::Static("hello".to_string());
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#""hello""#);
    }
    
    #[test]
    fn test_serialize_env_var() {
        let value: ConfigValue<String> = ConfigValue::EnvironmentVariable {
            name: "MY_VAR".to_string(),
            default: Some("default".to_string()),
        };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["kind"], "EnvironmentVariable");
        assert_eq!(json["name"], "MY_VAR");
        assert_eq!(json["default"], "default");
    }
    
    #[test]
    fn test_serialize_secret() {
        let value: ConfigValue<String> = ConfigValue::Secret {
            name: "my-secret".to_string(),
        };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["kind"], "Secret");
        assert_eq!(json["name"], "my-secret");
    }
}
