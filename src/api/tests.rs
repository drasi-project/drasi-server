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

#[cfg(test)]
mod handler_tests {
    use super::super::*;
    use drasi_server_core::{
        SourceManager, QueryManager, ReactionManager,
        channels::EventChannels,
        ComponentStatus,
        SourceConfig, QueryConfig, ReactionConfig
    };
    use serde_json::json;
    use std::sync::Arc;
    
    async fn create_test_managers() -> (Arc<SourceManager>, Arc<QueryManager>, Arc<ReactionManager>) {
        let (channels, _receivers) = EventChannels::new();
        
        let source_manager = Arc::new(SourceManager::new(
            channels.source_change_tx.clone(),
            channels.component_event_tx.clone(),
        ));
        
        let query_manager = Arc::new(QueryManager::new(
            channels.query_result_tx.clone(),
            channels.component_event_tx.clone(),
            channels.bootstrap_request_tx.clone(),
        ));
        
        let reaction_manager = Arc::new(ReactionManager::new(
            channels.component_event_tx.clone(),
        ));
        
        (source_manager, query_manager, reaction_manager)
    }
    
    #[tokio::test]
    async fn test_api_response_constructors() {
        // Test success constructor
        let response = ApiResponse::success("test data".to_string());
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"], "test data");
        assert!(json["error"].is_null());
        
        // Test error constructor
        let error_response: ApiResponse<String> = ApiResponse::error("Something went wrong".to_string());
        let json = serde_json::to_value(&error_response).unwrap();
        assert_eq!(json["success"], false);
        assert!(json["data"].is_null());
        assert_eq!(json["error"], "Something went wrong");
    }
    
    #[tokio::test]
    async fn test_component_status_serialization() {
        // Test that ComponentStatus can be serialized
        let status = ComponentStatus::Running;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, "Running");
        
        let stopped = ComponentStatus::Stopped;
        let json = serde_json::to_value(&stopped).unwrap();
        assert_eq!(json, "Stopped");
    }
    
    
    #[tokio::test]
    async fn test_source_manager_api_operations() {
        let (source_manager, _, _) = create_test_managers().await;
        
        // Test adding a source
        let config = SourceConfig {
            id: "test-source".to_string(),
            source_type: "internal.mock".to_string(),
            auto_start: false,
            properties: std::collections::HashMap::new(),
        };
        
        let result = source_manager.add_source(config.clone()).await;
        assert!(result.is_ok());
        
        // Test listing sources
        let sources = source_manager.list_sources().await;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].0, "test-source");
        
        // Test getting a source
        let retrieved = source_manager.get_source_config("test-source").await;
        assert!(retrieved.is_some());
        
        // Test updating a source
        let mut updated_config = config.clone();
        updated_config.properties.insert("new_prop".to_string(), json!("value"));
        
        let update_result = source_manager.update_source("test-source".to_string(), updated_config).await;
        assert!(update_result.is_ok());
        
        // Test deleting a source
        let delete_result = source_manager.delete_source("test-source".to_string()).await;
        assert!(delete_result.is_ok());
    }
    
    #[tokio::test]
    async fn test_query_manager_api_operations() {
        let (_, query_manager, _) = create_test_managers().await;
        
        // Test adding a query
        let config = QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string()],
            auto_start: false,
            properties: std::collections::HashMap::new(),
            joins: None,
        };
        
        let result = query_manager.add_query(config.clone()).await;
        assert!(result.is_ok());
        
        // Test listing queries
        let queries = query_manager.list_queries().await;
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].0, "test-query");
        
        // Test getting a query
        let retrieved = query_manager.get_query_config("test-query").await;
        assert!(retrieved.is_some());
        
        // Test deleting a query
        let delete_result = query_manager.delete_query("test-query".to_string()).await;
        assert!(delete_result.is_ok());
    }
    
    #[tokio::test]
    async fn test_reaction_manager_api_operations() {
        let (_, _, reaction_manager) = create_test_managers().await;
        
        // Test adding a reaction
        let config = ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query1".to_string()],
            auto_start: false,
            properties: std::collections::HashMap::new(),
        };
        
        let result = reaction_manager.add_reaction(config.clone()).await;
        assert!(result.is_ok());
        
        // Test listing reactions
        let reactions = reaction_manager.list_reactions().await;
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].0, "test-reaction");
        
        // Test getting a reaction
        let retrieved = reaction_manager.get_reaction_config("test-reaction").await;
        assert!(retrieved.is_some());
        
        // Test deleting a reaction
        let delete_result = reaction_manager.delete_reaction("test-reaction".to_string()).await;
        assert!(delete_result.is_ok());
    }
}

#[cfg(test)]
mod serialization_tests {
    use drasi_server_core::{SourceConfig, QueryConfig, ReactionConfig};
    use serde_json::json;
    
    #[test]
    fn test_source_config_json_serialization() {
        let config = SourceConfig {
            id: "test-source".to_string(),
            source_type: "internal.mock".to_string(),
            auto_start: true,
            properties: std::collections::HashMap::from([
                ("key".to_string(), json!("value"))
            ]),
        };
        
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-source");
        assert_eq!(json["source_type"], "internal.mock");
        assert_eq!(json["auto_start"], true);
        assert_eq!(json["properties"]["key"], "value");
        
        // Test deserialization
        let deserialized: SourceConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, config.id);
    }
    
    #[test]
    fn test_query_config_json_serialization() {
        let config = QueryConfig {
            id: "test-query".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            sources: vec!["source1".to_string(), "source2".to_string()],
            auto_start: false,
            properties: std::collections::HashMap::new(),
            joins: None,
        };
        
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-query");
        assert_eq!(json["query"], "MATCH (n) RETURN n");
        assert_eq!(json["sources"].as_array().unwrap().len(), 2);
        
        // Test deserialization
        let deserialized: QueryConfig = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.sources.len(), 2);
    }
    
    #[test]
    fn test_reaction_config_json_serialization() {
        let config = ReactionConfig {
            id: "test-reaction".to_string(),
            reaction_type: "log".to_string(),
            queries: vec!["query1".to_string()],
            auto_start: true,
            properties: std::collections::HashMap::from([
                ("log_level".to_string(), json!("info"))
            ]),
        };
        
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], "test-reaction");
        assert_eq!(json["reaction_type"], "log");
        assert_eq!(json["queries"].as_array().unwrap().len(), 1);
        assert_eq!(json["properties"]["log_level"], "info");
    }
}