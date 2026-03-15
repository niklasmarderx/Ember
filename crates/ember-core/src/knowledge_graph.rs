//! # Knowledge Graph
//!
//! A semantic knowledge storage and retrieval system that enables Ember agents
//! to build, maintain, and query structured knowledge about entities, relationships,
//! and concepts.
//!
//! ## Features
//!
//! - **Entity Management**: Create, update, and delete entities with properties
//! - **Relationship Tracking**: Define typed relationships between entities
//! - **Semantic Search**: Find entities and relationships using natural language
//! - **Graph Traversal**: Navigate relationships to discover connected knowledge
//! - **Temporal Awareness**: Track when knowledge was added and modified
//! - **Confidence Scoring**: Track reliability of information
//!
//! ## Example
//!
//! ```rust,ignore
//! use ember_core::knowledge_graph::{KnowledgeGraph, Entity, Relationship};
//!
//! let mut graph = KnowledgeGraph::new();
//!
//! // Add entities
//! let rust = graph.add_entity(
//!     Entity::new("Rust", "ProgrammingLanguage")
//!         .with_property("paradigm", "systems")
//!         .with_property("memory_safe", true)
//! );
//!
//! let tokio = graph.add_entity(
//!     Entity::new("Tokio", "Library")
//!         .with_property("purpose", "async runtime")
//! );
//!
//! // Create relationship
//! graph.add_relationship(tokio, "written_in", rust);
//!
//! // Query the graph
//! let results = graph.query("What libraries are written in Rust?").await?;
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{Error, Result};

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for an entity in the knowledge graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(Uuid);

impl EntityId {
    /// Create a new unique entity ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the inner UUID
    #[must_use]
    pub fn inner(&self) -> Uuid {
        self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a relationship
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelationshipId(Uuid);

impl RelationshipId {
    /// Create a new unique relationship ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RelationshipId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RelationshipId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A value that can be stored as an entity property
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    /// String value
    String(String),
    /// Integer value
    Integer(i64),
    /// Float value
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// List of values
    List(Vec<PropertyValue>),
    /// Nested object
    Object(HashMap<String, PropertyValue>),
    /// Null value
    Null,
}

impl From<&str> for PropertyValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for PropertyValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i64> for PropertyValue {
    fn from(n: i64) -> Self {
        Self::Integer(n)
    }
}

impl From<i32> for PropertyValue {
    fn from(n: i32) -> Self {
        Self::Integer(n as i64)
    }
}

impl From<f64> for PropertyValue {
    fn from(n: f64) -> Self {
        Self::Float(n)
    }
}

impl From<bool> for PropertyValue {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

impl<T: Into<PropertyValue>> From<Vec<T>> for PropertyValue {
    fn from(v: Vec<T>) -> Self {
        Self::List(v.into_iter().map(Into::into).collect())
    }
}

// ============================================================================
// Entity
// ============================================================================

/// An entity in the knowledge graph representing a concept, object, or idea
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier
    pub id: EntityId,
    /// Human-readable name
    pub name: String,
    /// Entity type/category
    pub entity_type: String,
    /// Properties associated with this entity
    pub properties: HashMap<String, PropertyValue>,
    /// Tags for categorization
    pub tags: HashSet<String>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// When the entity was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the entity was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Source of this information
    pub source: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Entity {
    /// Create a new entity with the given name and type
    #[must_use]
    pub fn new(name: impl Into<String>, entity_type: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: EntityId::new(),
            name: name.into(),
            entity_type: entity_type.into(),
            properties: HashMap::new(),
            tags: HashSet::new(),
            confidence: 1.0,
            created_at: now,
            updated_at: now,
            source: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a property to the entity
    #[must_use]
    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Add a tag
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Set confidence score
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set source
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Get a property value
    #[must_use]
    pub fn get_property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// Check if entity has a specific tag
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    /// Update a property
    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<PropertyValue>) {
        self.properties.insert(key.into(), value.into());
        self.updated_at = chrono::Utc::now();
    }
}

// ============================================================================
// Relationship
// ============================================================================

/// The direction of a relationship for traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationDirection {
    /// From source to target
    Outgoing,
    /// From target to source
    Incoming,
    /// Either direction
    Both,
}

/// A relationship between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Unique identifier
    pub id: RelationshipId,
    /// Source entity ID
    pub source: EntityId,
    /// Target entity ID
    pub target: EntityId,
    /// Type of relationship (e.g., "written_in", "depends_on", "part_of")
    pub relation_type: String,
    /// Properties of this relationship
    pub properties: HashMap<String, PropertyValue>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Weight/strength of the relationship
    pub weight: f64,
    /// When the relationship was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Source of this information
    pub source_info: Option<String>,
}

impl Relationship {
    /// Create a new relationship
    #[must_use]
    pub fn new(source: EntityId, relation_type: impl Into<String>, target: EntityId) -> Self {
        Self {
            id: RelationshipId::new(),
            source,
            target,
            relation_type: relation_type.into(),
            properties: HashMap::new(),
            confidence: 1.0,
            weight: 1.0,
            created_at: chrono::Utc::now(),
            source_info: None,
        }
    }

    /// Add a property
    #[must_use]
    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<PropertyValue>,
    ) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Set confidence
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set weight
    #[must_use]
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// Set source info
    #[must_use]
    pub fn with_source_info(mut self, source: impl Into<String>) -> Self {
        self.source_info = Some(source.into());
        self
    }
}

// ============================================================================
// Query Types
// ============================================================================

/// A query against the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    /// Entity type filter
    pub entity_type: Option<String>,
    /// Property filters
    pub property_filters: Vec<PropertyFilter>,
    /// Tag filters (entity must have all)
    pub required_tags: HashSet<String>,
    /// Relationship type filter
    pub relationship_type: Option<String>,
    /// Minimum confidence threshold
    pub min_confidence: f64,
    /// Maximum results
    pub limit: usize,
    /// Text search query
    pub text_query: Option<String>,
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self {
            entity_type: None,
            property_filters: Vec::new(),
            required_tags: HashSet::new(),
            relationship_type: None,
            min_confidence: 0.0,
            limit: 100,
            text_query: None,
        }
    }
}

impl GraphQuery {
    /// Create a new query builder
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by entity type
    #[must_use]
    pub fn entity_type(mut self, entity_type: impl Into<String>) -> Self {
        self.entity_type = Some(entity_type.into());
        self
    }

    /// Add a property filter
    #[must_use]
    pub fn property_filter(mut self, filter: PropertyFilter) -> Self {
        self.property_filters.push(filter);
        self
    }

    /// Require a tag
    #[must_use]
    pub fn require_tag(mut self, tag: impl Into<String>) -> Self {
        self.required_tags.insert(tag.into());
        self
    }

    /// Filter by relationship type
    #[must_use]
    pub fn relationship_type(mut self, rel_type: impl Into<String>) -> Self {
        self.relationship_type = Some(rel_type.into());
        self
    }

    /// Set minimum confidence
    #[must_use]
    pub fn min_confidence(mut self, confidence: f64) -> Self {
        self.min_confidence = confidence;
        self
    }

    /// Set result limit
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set text query
    #[must_use]
    pub fn text(mut self, query: impl Into<String>) -> Self {
        self.text_query = Some(query.into());
        self
    }
}

/// Filter for entity properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyFilter {
    /// Property key
    pub key: String,
    /// Filter operation
    pub operation: FilterOperation,
    /// Value to compare against
    pub value: PropertyValue,
}

impl PropertyFilter {
    /// Create an equality filter
    #[must_use]
    pub fn equals(key: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        Self {
            key: key.into(),
            operation: FilterOperation::Equals,
            value: value.into(),
        }
    }

    /// Create a contains filter
    #[must_use]
    pub fn contains(key: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        Self {
            key: key.into(),
            operation: FilterOperation::Contains,
            value: value.into(),
        }
    }

    /// Create a greater than filter
    #[must_use]
    pub fn greater_than(key: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        Self {
            key: key.into(),
            operation: FilterOperation::GreaterThan,
            value: value.into(),
        }
    }

    /// Create a less than filter
    #[must_use]
    pub fn less_than(key: impl Into<String>, value: impl Into<PropertyValue>) -> Self {
        Self {
            key: key.into(),
            operation: FilterOperation::LessThan,
            value: value.into(),
        }
    }
}

/// Filter operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOperation {
    /// Exact equality
    Equals,
    /// Not equal
    NotEquals,
    /// Greater than (numeric)
    GreaterThan,
    /// Less than (numeric)
    LessThan,
    /// Contains (string or list)
    Contains,
    /// Starts with (string)
    StartsWith,
    /// Ends with (string)
    EndsWith,
    /// Exists (property is present)
    Exists,
}

/// Result of a graph query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Matching entities
    pub entities: Vec<Entity>,
    /// Related relationships
    pub relationships: Vec<Relationship>,
    /// Total count (may be more than returned due to limit)
    pub total_count: usize,
    /// Query execution time
    pub execution_time: Duration,
}

// ============================================================================
// Graph Traversal
// ============================================================================

/// Options for graph traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalOptions {
    /// Maximum depth to traverse
    pub max_depth: usize,
    /// Relationship types to follow (empty = all)
    pub relationship_types: HashSet<String>,
    /// Direction to traverse
    pub direction: RelationDirection,
    /// Maximum entities to visit
    pub max_entities: usize,
    /// Minimum confidence for relationships
    pub min_confidence: f64,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            relationship_types: HashSet::new(),
            direction: RelationDirection::Both,
            max_entities: 100,
            min_confidence: 0.0,
        }
    }
}

impl TraversalOptions {
    /// Create new traversal options
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set max depth
    #[must_use]
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Add relationship type to follow
    #[must_use]
    pub fn follow_relationship(mut self, rel_type: impl Into<String>) -> Self {
        self.relationship_types.insert(rel_type.into());
        self
    }

    /// Set traversal direction
    #[must_use]
    pub fn direction(mut self, direction: RelationDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set max entities
    #[must_use]
    pub fn max_entities(mut self, max: usize) -> Self {
        self.max_entities = max;
        self
    }
}

/// Result of graph traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalResult {
    /// Starting entity
    pub start: EntityId,
    /// Visited entities with their depth
    pub visited: Vec<(Entity, usize)>,
    /// Traversed relationships
    pub relationships: Vec<Relationship>,
    /// Path from start to each visited entity
    pub paths: HashMap<EntityId, Vec<RelationshipId>>,
}

// ============================================================================
// Knowledge Graph
// ============================================================================

/// Configuration for the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    /// Maximum entities to store
    pub max_entities: usize,
    /// Maximum relationships to store
    pub max_relationships: usize,
    /// Enable automatic cleanup of low-confidence entries
    pub auto_cleanup: bool,
    /// Minimum confidence to keep during cleanup
    pub cleanup_threshold: f64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            max_entities: 10000,
            max_relationships: 50000,
            auto_cleanup: true,
            cleanup_threshold: 0.1,
        }
    }
}

/// Statistics about the knowledge graph
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total entities
    pub entity_count: usize,
    /// Total relationships
    pub relationship_count: usize,
    /// Entities by type
    pub entities_by_type: HashMap<String, usize>,
    /// Relationships by type
    pub relationships_by_type: HashMap<String, usize>,
    /// Average entity confidence
    pub avg_entity_confidence: f64,
    /// Average relationship confidence
    pub avg_relationship_confidence: f64,
    /// Total queries executed
    pub queries_executed: u64,
}

/// The main knowledge graph structure
pub struct KnowledgeGraph {
    config: GraphConfig,
    entities: RwLock<HashMap<EntityId, Entity>>,
    relationships: RwLock<HashMap<RelationshipId, Relationship>>,
    // Index: entity -> outgoing relationships
    outgoing: RwLock<HashMap<EntityId, Vec<RelationshipId>>>,
    // Index: entity -> incoming relationships
    incoming: RwLock<HashMap<EntityId, Vec<RelationshipId>>>,
    // Index: entity type -> entity IDs
    type_index: RwLock<HashMap<String, HashSet<EntityId>>>,
    // Index: relationship type -> relationship IDs
    rel_type_index: RwLock<HashMap<String, HashSet<RelationshipId>>>,
    // Index: name -> entity ID (for fast lookup)
    name_index: RwLock<HashMap<String, EntityId>>,
    stats: RwLock<GraphStats>,
}

impl KnowledgeGraph {
    /// Create a new knowledge graph with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(GraphConfig::default())
    }

    /// Create a new knowledge graph with custom configuration
    #[must_use]
    pub fn with_config(config: GraphConfig) -> Self {
        Self {
            config,
            entities: RwLock::new(HashMap::new()),
            relationships: RwLock::new(HashMap::new()),
            outgoing: RwLock::new(HashMap::new()),
            incoming: RwLock::new(HashMap::new()),
            type_index: RwLock::new(HashMap::new()),
            rel_type_index: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            stats: RwLock::new(GraphStats::default()),
        }
    }

    /// Add an entity to the graph
    pub async fn add_entity(&self, entity: Entity) -> Result<EntityId> {
        let entities = self.entities.read().await;
        if entities.len() >= self.config.max_entities {
            return Err(Error::ResourceExhausted(format!(
                "Maximum entities reached: {}",
                self.config.max_entities
            )));
        }
        drop(entities);

        let id = entity.id;
        let name = entity.name.clone();
        let entity_type = entity.entity_type.clone();

        // Add to main store
        {
            let mut entities = self.entities.write().await;
            entities.insert(id, entity);
        }

        // Update type index
        {
            let mut type_idx = self.type_index.write().await;
            type_idx.entry(entity_type.clone()).or_default().insert(id);
        }

        // Update name index
        {
            let mut name_idx = self.name_index.write().await;
            name_idx.insert(name.to_lowercase(), id);
        }

        // Initialize relationship indices
        {
            let mut out = self.outgoing.write().await;
            out.entry(id).or_default();
        }
        {
            let mut inc = self.incoming.write().await;
            inc.entry(id).or_default();
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.entity_count += 1;
            *stats
                .entities_by_type
                .entry(entity_type.clone())
                .or_default() += 1;
        }

        debug!(entity_id = %id, "Added entity to knowledge graph");

        Ok(id)
    }

    /// Get an entity by ID
    pub async fn get_entity(&self, id: EntityId) -> Option<Entity> {
        let entities = self.entities.read().await;
        entities.get(&id).cloned()
    }

    /// Get an entity by name
    pub async fn get_entity_by_name(&self, name: &str) -> Option<Entity> {
        let name_idx = self.name_index.read().await;
        if let Some(&id) = name_idx.get(&name.to_lowercase()) {
            self.get_entity(id).await
        } else {
            None
        }
    }

    /// Update an entity
    pub async fn update_entity(
        &self,
        id: EntityId,
        update_fn: impl FnOnce(&mut Entity),
    ) -> Result<()> {
        let mut entities = self.entities.write().await;
        if let Some(entity) = entities.get_mut(&id) {
            update_fn(entity);
            entity.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(Error::NotFound(format!("Entity not found: {}", id)))
        }
    }

    /// Remove an entity and all its relationships
    pub async fn remove_entity(&self, id: EntityId) -> Result<Entity> {
        // Get the entity
        let entity = {
            let mut entities = self.entities.write().await;
            entities
                .remove(&id)
                .ok_or_else(|| Error::NotFound(format!("Entity not found: {}", id)))?
        };

        // Remove from type index
        {
            let mut type_idx = self.type_index.write().await;
            if let Some(set) = type_idx.get_mut(&entity.entity_type) {
                set.remove(&id);
            }
        }

        // Remove from name index
        {
            let mut name_idx = self.name_index.write().await;
            name_idx.remove(&entity.name.to_lowercase());
        }

        // Remove relationships
        let rels_to_remove: Vec<RelationshipId> = {
            let out = self.outgoing.read().await;
            let inc = self.incoming.read().await;

            let mut rels = Vec::new();
            if let Some(outgoing) = out.get(&id) {
                rels.extend(outgoing.iter().copied());
            }
            if let Some(incoming) = inc.get(&id) {
                rels.extend(incoming.iter().copied());
            }
            rels
        };

        for rel_id in rels_to_remove {
            let _ = self.remove_relationship(rel_id).await;
        }

        // Remove from relationship indices
        {
            let mut out = self.outgoing.write().await;
            out.remove(&id);
        }
        {
            let mut inc = self.incoming.write().await;
            inc.remove(&id);
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.entity_count = stats.entity_count.saturating_sub(1);
            if let Some(count) = stats.entities_by_type.get_mut(&entity.entity_type) {
                *count = count.saturating_sub(1);
            }
        }

        Ok(entity)
    }

    /// Add a relationship between two entities
    pub async fn add_relationship(&self, relationship: Relationship) -> Result<RelationshipId> {
        // Verify both entities exist
        {
            let entities = self.entities.read().await;
            if !entities.contains_key(&relationship.source) {
                return Err(Error::NotFound(format!(
                    "Source entity not found: {}",
                    relationship.source
                )));
            }
            if !entities.contains_key(&relationship.target) {
                return Err(Error::NotFound(format!(
                    "Target entity not found: {}",
                    relationship.target
                )));
            }
        }

        // Check capacity
        {
            let rels = self.relationships.read().await;
            if rels.len() >= self.config.max_relationships {
                return Err(Error::ResourceExhausted(format!(
                    "Maximum relationships reached: {}",
                    self.config.max_relationships
                )));
            }
        }

        let id = relationship.id;
        let source = relationship.source;
        let target = relationship.target;
        let rel_type = relationship.relation_type.clone();

        // Add to main store
        {
            let mut rels = self.relationships.write().await;
            rels.insert(id, relationship);
        }

        // Update indices
        {
            let mut out = self.outgoing.write().await;
            out.entry(source).or_default().push(id);
        }
        {
            let mut inc = self.incoming.write().await;
            inc.entry(target).or_default().push(id);
        }
        {
            let mut rel_idx = self.rel_type_index.write().await;
            rel_idx.entry(rel_type.clone()).or_default().insert(id);
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.relationship_count += 1;
            *stats.relationships_by_type.entry(rel_type).or_default() += 1;
        }

        debug!(rel_id = %id, source = %source, target = %target, "Added relationship");

        Ok(id)
    }

    /// Create a relationship using source, type, and target
    pub async fn relate(
        &self,
        source: EntityId,
        relation_type: impl Into<String>,
        target: EntityId,
    ) -> Result<RelationshipId> {
        let rel = Relationship::new(source, relation_type, target);
        self.add_relationship(rel).await
    }

    /// Get a relationship by ID
    pub async fn get_relationship(&self, id: RelationshipId) -> Option<Relationship> {
        let rels = self.relationships.read().await;
        rels.get(&id).cloned()
    }

    /// Remove a relationship
    pub async fn remove_relationship(&self, id: RelationshipId) -> Result<Relationship> {
        let rel = {
            let mut rels = self.relationships.write().await;
            rels.remove(&id)
                .ok_or_else(|| Error::NotFound(format!("Relationship not found: {:?}", id)))?
        };

        // Update indices
        {
            let mut out = self.outgoing.write().await;
            if let Some(list) = out.get_mut(&rel.source) {
                list.retain(|&r| r != id);
            }
        }
        {
            let mut inc = self.incoming.write().await;
            if let Some(list) = inc.get_mut(&rel.target) {
                list.retain(|&r| r != id);
            }
        }
        {
            let mut rel_idx = self.rel_type_index.write().await;
            if let Some(set) = rel_idx.get_mut(&rel.relation_type) {
                set.remove(&id);
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.relationship_count = stats.relationship_count.saturating_sub(1);
            if let Some(count) = stats.relationships_by_type.get_mut(&rel.relation_type) {
                *count = count.saturating_sub(1);
            }
        }

        Ok(rel)
    }

    /// Get all relationships for an entity
    pub async fn get_relationships(
        &self,
        entity_id: EntityId,
        direction: RelationDirection,
    ) -> Vec<Relationship> {
        let mut rel_ids = Vec::new();

        if matches!(
            direction,
            RelationDirection::Outgoing | RelationDirection::Both
        ) {
            let out = self.outgoing.read().await;
            if let Some(ids) = out.get(&entity_id) {
                rel_ids.extend(ids.iter().copied());
            }
        }

        if matches!(
            direction,
            RelationDirection::Incoming | RelationDirection::Both
        ) {
            let inc = self.incoming.read().await;
            if let Some(ids) = inc.get(&entity_id) {
                rel_ids.extend(ids.iter().copied());
            }
        }

        let rels = self.relationships.read().await;
        rel_ids
            .iter()
            .filter_map(|id| rels.get(id).cloned())
            .collect()
    }

    /// Get neighbors of an entity
    pub async fn get_neighbors(
        &self,
        entity_id: EntityId,
        direction: RelationDirection,
    ) -> Vec<Entity> {
        let rels = self.get_relationships(entity_id, direction).await;
        let entities = self.entities.read().await;

        let mut neighbor_ids: HashSet<EntityId> = HashSet::new();
        for rel in &rels {
            if rel.source == entity_id {
                neighbor_ids.insert(rel.target);
            } else {
                neighbor_ids.insert(rel.source);
            }
        }

        neighbor_ids
            .iter()
            .filter_map(|id| entities.get(id).cloned())
            .collect()
    }

    /// Query the knowledge graph
    pub async fn query(&self, query: GraphQuery) -> QueryResult {
        let start = std::time::Instant::now();

        let entities = self.entities.read().await;
        let relationships = self.relationships.read().await;

        // Filter entities
        let mut matching_entities: Vec<Entity> = entities
            .values()
            .filter(|e| {
                // Type filter
                if let Some(ref entity_type) = query.entity_type {
                    if &e.entity_type != entity_type {
                        return false;
                    }
                }

                // Confidence filter
                if e.confidence < query.min_confidence {
                    return false;
                }

                // Tag filter
                for tag in &query.required_tags {
                    if !e.tags.contains(tag) {
                        return false;
                    }
                }

                // Property filters
                for filter in &query.property_filters {
                    if !Self::check_property_filter(e, filter) {
                        return false;
                    }
                }

                // Text query (simple name/type match)
                if let Some(ref text) = query.text_query {
                    let lower_text = text.to_lowercase();
                    let name_lower = e.name.to_lowercase();
                    let type_lower = e.entity_type.to_lowercase();
                    if !name_lower.contains(&lower_text) && !type_lower.contains(&lower_text) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        let total_count = matching_entities.len();

        // Apply limit
        matching_entities.truncate(query.limit);

        // Get related relationships
        let entity_ids: HashSet<EntityId> = matching_entities.iter().map(|e| e.id).collect();
        let related_rels: Vec<Relationship> = relationships
            .values()
            .filter(|r| {
                // Either source or target in our results
                (entity_ids.contains(&r.source) || entity_ids.contains(&r.target))
                    // Optional relationship type filter
                    && query.relationship_type.as_ref()
                        .map(|rt| &r.relation_type == rt)
                        .unwrap_or(true)
            })
            .cloned()
            .collect();

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.queries_executed += 1;
        }

        QueryResult {
            entities: matching_entities,
            relationships: related_rels,
            total_count,
            execution_time: start.elapsed(),
        }
    }

    /// Check if an entity passes a property filter
    fn check_property_filter(entity: &Entity, filter: &PropertyFilter) -> bool {
        let Some(prop) = entity.properties.get(&filter.key) else {
            return matches!(filter.operation, FilterOperation::Exists);
        };

        match filter.operation {
            FilterOperation::Equals => prop == &filter.value,
            FilterOperation::NotEquals => prop != &filter.value,
            FilterOperation::Contains => match (prop, &filter.value) {
                (PropertyValue::String(s), PropertyValue::String(v)) => s.contains(v),
                (PropertyValue::List(l), v) => l.contains(v),
                _ => false,
            },
            FilterOperation::StartsWith => match (prop, &filter.value) {
                (PropertyValue::String(s), PropertyValue::String(v)) => s.starts_with(v),
                _ => false,
            },
            FilterOperation::EndsWith => match (prop, &filter.value) {
                (PropertyValue::String(s), PropertyValue::String(v)) => s.ends_with(v),
                _ => false,
            },
            FilterOperation::GreaterThan => match (prop, &filter.value) {
                (PropertyValue::Integer(a), PropertyValue::Integer(b)) => a > b,
                (PropertyValue::Float(a), PropertyValue::Float(b)) => a > b,
                _ => false,
            },
            FilterOperation::LessThan => match (prop, &filter.value) {
                (PropertyValue::Integer(a), PropertyValue::Integer(b)) => a < b,
                (PropertyValue::Float(a), PropertyValue::Float(b)) => a < b,
                _ => false,
            },
            FilterOperation::Exists => true,
        }
    }

    /// Traverse the graph from a starting entity
    pub async fn traverse(
        &self,
        start: EntityId,
        options: TraversalOptions,
    ) -> Result<TraversalResult> {
        let entities = self.entities.read().await;
        let relationships = self.relationships.read().await;

        if !entities.contains_key(&start) {
            return Err(Error::NotFound(format!(
                "Start entity not found: {}",
                start
            )));
        }

        let mut visited: Vec<(Entity, usize)> = Vec::new();
        let mut visited_ids: HashSet<EntityId> = HashSet::new();
        let mut traversed_rels: Vec<Relationship> = Vec::new();
        let mut paths: HashMap<EntityId, Vec<RelationshipId>> = HashMap::new();

        // BFS traversal
        let mut queue: VecDeque<(EntityId, usize, Vec<RelationshipId>)> = VecDeque::new();
        queue.push_back((start, 0, Vec::new()));
        visited_ids.insert(start);

        while let Some((current, depth, path)) = queue.pop_front() {
            if visited.len() >= options.max_entities {
                break;
            }

            if let Some(entity) = entities.get(&current) {
                visited.push((entity.clone(), depth));
                paths.insert(current, path.clone());
            }

            if depth >= options.max_depth {
                continue;
            }

            // Get neighbors based on direction
            let mut neighbor_rels: Vec<&Relationship> = Vec::new();

            if matches!(
                options.direction,
                RelationDirection::Outgoing | RelationDirection::Both
            ) {
                let out = self.outgoing.read().await;
                if let Some(rel_ids) = out.get(&current) {
                    for rel_id in rel_ids {
                        if let Some(rel) = relationships.get(rel_id) {
                            neighbor_rels.push(rel);
                        }
                    }
                }
            }

            if matches!(
                options.direction,
                RelationDirection::Incoming | RelationDirection::Both
            ) {
                let inc = self.incoming.read().await;
                if let Some(rel_ids) = inc.get(&current) {
                    for rel_id in rel_ids {
                        if let Some(rel) = relationships.get(rel_id) {
                            neighbor_rels.push(rel);
                        }
                    }
                }
            }

            for rel in neighbor_rels {
                // Check relationship type filter
                if !options.relationship_types.is_empty()
                    && !options.relationship_types.contains(&rel.relation_type)
                {
                    continue;
                }

                // Check confidence
                if rel.confidence < options.min_confidence {
                    continue;
                }

                let neighbor = if rel.source == current {
                    rel.target
                } else {
                    rel.source
                };

                if visited_ids.insert(neighbor) {
                    let mut new_path = path.clone();
                    new_path.push(rel.id);
                    queue.push_back((neighbor, depth + 1, new_path));
                    traversed_rels.push(rel.clone());
                }
            }
        }

        Ok(TraversalResult {
            start,
            visited,
            relationships: traversed_rels,
            paths,
        })
    }

    /// Find shortest path between two entities
    pub async fn find_path(
        &self,
        from: EntityId,
        to: EntityId,
        options: TraversalOptions,
    ) -> Result<Option<Vec<Relationship>>> {
        let result = self.traverse(from, options).await?;

        if let Some(path_ids) = result.paths.get(&to) {
            let relationships = self.relationships.read().await;
            let path: Vec<Relationship> = path_ids
                .iter()
                .filter_map(|id| relationships.get(id).cloned())
                .collect();

            if path.len() == path_ids.len() {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }

    /// Get entities of a specific type
    pub async fn get_entities_by_type(&self, entity_type: &str) -> Vec<Entity> {
        let type_idx = self.type_index.read().await;
        let entities = self.entities.read().await;

        if let Some(ids) = type_idx.get(entity_type) {
            ids.iter()
                .filter_map(|id| entities.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get graph statistics
    pub async fn stats(&self) -> GraphStats {
        let mut stats = self.stats.read().await.clone();

        // Calculate average confidences
        let entities = self.entities.read().await;
        if !entities.is_empty() {
            let total: f64 = entities.values().map(|e| e.confidence).sum();
            stats.avg_entity_confidence = total / entities.len() as f64;
        }

        let rels = self.relationships.read().await;
        if !rels.is_empty() {
            let total: f64 = rels.values().map(|r| r.confidence).sum();
            stats.avg_relationship_confidence = total / rels.len() as f64;
        }

        stats
    }

    /// Clean up low-confidence entries
    pub async fn cleanup(&self, min_confidence: f64) -> (usize, usize) {
        let entities_removed;
        let rels_removed;

        // Find low-confidence entities
        let to_remove: Vec<EntityId> = {
            let entities = self.entities.read().await;
            entities
                .iter()
                .filter(|(_, e)| e.confidence < min_confidence)
                .map(|(id, _)| *id)
                .collect()
        };

        entities_removed = to_remove.len();
        for id in to_remove {
            let _ = self.remove_entity(id).await;
        }

        // Find low-confidence relationships
        let rels_to_remove: Vec<RelationshipId> = {
            let rels = self.relationships.read().await;
            rels.iter()
                .filter(|(_, r)| r.confidence < min_confidence)
                .map(|(id, _)| *id)
                .collect()
        };

        rels_removed = rels_to_remove.len();
        for id in rels_to_remove {
            let _ = self.remove_relationship(id).await;
        }

        info!(
            entities_removed = entities_removed,
            relationships_removed = rels_removed,
            "Knowledge graph cleanup complete"
        );

        (entities_removed, rels_removed)
    }

    /// Merge two entities into one
    pub async fn merge_entities(&self, keep: EntityId, remove: EntityId) -> Result<Entity> {
        // Get both entities
        let (_keep_entity, remove_entity) = {
            let entities = self.entities.read().await;
            let keep_e = entities
                .get(&keep)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("Entity not found: {}", keep)))?;
            let remove_e = entities
                .get(&remove)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("Entity not found: {}", remove)))?;
            (keep_e, remove_e)
        };

        // Merge properties from removed entity
        {
            let mut entities = self.entities.write().await;
            if let Some(entity) = entities.get_mut(&keep) {
                for (key, value) in remove_entity.properties {
                    entity.properties.entry(key).or_insert(value);
                }
                for tag in remove_entity.tags {
                    entity.tags.insert(tag);
                }
                entity.updated_at = chrono::Utc::now();
            }
        }

        // Redirect relationships from removed entity to kept entity
        let rels_to_update: Vec<(RelationshipId, bool)> = {
            let rels = self.relationships.read().await;
            rels.values()
                .filter_map(|r| {
                    if r.source == remove {
                        Some((r.id, true)) // Update source
                    } else if r.target == remove {
                        Some((r.id, false)) // Update target
                    } else {
                        None
                    }
                })
                .collect()
        };

        {
            let mut rels = self.relationships.write().await;
            for (rel_id, is_source) in rels_to_update {
                if let Some(rel) = rels.get_mut(&rel_id) {
                    if is_source {
                        rel.source = keep;
                    } else {
                        rel.target = keep;
                    }
                }
            }
        }

        // Remove the merged entity
        let _ = self.remove_entity(remove).await;

        // Return updated entity
        self.get_entity(keep)
            .await
            .ok_or_else(|| Error::Internal("Failed to get merged entity".into()))
    }

    /// Export the graph to a serializable format
    pub async fn export(&self) -> GraphExport {
        let entities = self.entities.read().await;
        let relationships = self.relationships.read().await;

        GraphExport {
            entities: entities.values().cloned().collect(),
            relationships: relationships.values().cloned().collect(),
            exported_at: chrono::Utc::now(),
        }
    }

    /// Import data into the graph
    pub async fn import(&self, data: GraphExport) -> Result<(usize, usize)> {
        let mut entities_added = 0;
        let mut rels_added = 0;

        // Import entities
        for entity in data.entities {
            if self.add_entity(entity).await.is_ok() {
                entities_added += 1;
            }
        }

        // Import relationships
        for rel in data.relationships {
            if self.add_relationship(rel).await.is_ok() {
                rels_added += 1;
            }
        }

        Ok((entities_added, rels_added))
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable export of the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExport {
    /// All entities
    pub entities: Vec<Entity>,
    /// All relationships
    pub relationships: Vec<Relationship>,
    /// When the export was created
    pub exported_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_entity() {
        let graph = KnowledgeGraph::new();

        let entity = Entity::new("Rust", "ProgrammingLanguage")
            .with_property("paradigm", "systems")
            .with_tag("fast");

        let id = graph.add_entity(entity).await.unwrap();
        let retrieved = graph.get_entity(id).await.unwrap();

        assert_eq!(retrieved.name, "Rust");
        assert_eq!(retrieved.entity_type, "ProgrammingLanguage");
        assert!(retrieved.has_tag("fast"));
    }

    #[tokio::test]
    async fn test_get_entity_by_name() {
        let graph = KnowledgeGraph::new();

        let entity = Entity::new("Tokio", "Library");
        graph.add_entity(entity).await.unwrap();

        let found = graph.get_entity_by_name("tokio").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Tokio");
    }

    #[tokio::test]
    async fn test_add_relationship() {
        let graph = KnowledgeGraph::new();

        let rust = Entity::new("Rust", "Language");
        let tokio = Entity::new("Tokio", "Library");

        let rust_id = graph.add_entity(rust).await.unwrap();
        let tokio_id = graph.add_entity(tokio).await.unwrap();

        graph.relate(tokio_id, "written_in", rust_id).await.unwrap();

        let neighbors = graph
            .get_neighbors(tokio_id, RelationDirection::Outgoing)
            .await;
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].name, "Rust");
    }

    #[tokio::test]
    async fn test_query() {
        let graph = KnowledgeGraph::new();

        graph
            .add_entity(Entity::new("Rust", "Language").with_tag("systems"))
            .await
            .unwrap();
        graph
            .add_entity(Entity::new("Python", "Language").with_tag("scripting"))
            .await
            .unwrap();
        graph
            .add_entity(Entity::new("Tokio", "Library"))
            .await
            .unwrap();

        let query = GraphQuery::new()
            .entity_type("Language")
            .require_tag("systems");

        let result = graph.query(query).await;

        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.entities[0].name, "Rust");
    }

    #[tokio::test]
    async fn test_traversal() {
        let graph = KnowledgeGraph::new();

        let a = graph.add_entity(Entity::new("A", "Node")).await.unwrap();
        let b = graph.add_entity(Entity::new("B", "Node")).await.unwrap();
        let c = graph.add_entity(Entity::new("C", "Node")).await.unwrap();
        let d = graph.add_entity(Entity::new("D", "Node")).await.unwrap();

        graph.relate(a, "connects_to", b).await.unwrap();
        graph.relate(b, "connects_to", c).await.unwrap();
        graph.relate(c, "connects_to", d).await.unwrap();

        let options = TraversalOptions::new()
            .max_depth(3)
            .direction(RelationDirection::Outgoing);

        let result = graph.traverse(a, options).await.unwrap();

        assert_eq!(result.visited.len(), 4);
    }

    #[tokio::test]
    async fn test_find_path() {
        let graph = KnowledgeGraph::new();

        let a = graph.add_entity(Entity::new("A", "Node")).await.unwrap();
        let b = graph.add_entity(Entity::new("B", "Node")).await.unwrap();
        let c = graph.add_entity(Entity::new("C", "Node")).await.unwrap();

        graph.relate(a, "to", b).await.unwrap();
        graph.relate(b, "to", c).await.unwrap();

        let path = graph
            .find_path(a, c, TraversalOptions::default())
            .await
            .unwrap();

        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_property_values() {
        let entity = Entity::new("Test", "Type")
            .with_property("string", "hello")
            .with_property("int", 42i64)
            .with_property("float", 3.14f64)
            .with_property("bool", true);

        assert!(matches!(
            entity.get_property("string"),
            Some(PropertyValue::String(_))
        ));
        assert!(matches!(
            entity.get_property("int"),
            Some(PropertyValue::Integer(42))
        ));
        assert!(matches!(
            entity.get_property("bool"),
            Some(PropertyValue::Boolean(true))
        ));
    }

    #[tokio::test]
    async fn test_remove_entity() {
        let graph = KnowledgeGraph::new();

        let entity = Entity::new("ToRemove", "Type");
        let id = graph.add_entity(entity).await.unwrap();

        assert!(graph.get_entity(id).await.is_some());

        graph.remove_entity(id).await.unwrap();

        assert!(graph.get_entity(id).await.is_none());
    }

    #[tokio::test]
    async fn test_merge_entities() {
        let graph = KnowledgeGraph::new();

        let e1 = Entity::new("Entity1", "Type").with_property("a", "1");
        let e2 = Entity::new("Entity2", "Type").with_property("b", "2");

        let id1 = graph.add_entity(e1).await.unwrap();
        let id2 = graph.add_entity(e2).await.unwrap();

        let merged = graph.merge_entities(id1, id2).await.unwrap();

        // Merged entity should have both properties
        assert!(merged.get_property("a").is_some());
        assert!(merged.get_property("b").is_some());

        // Second entity should be gone
        assert!(graph.get_entity(id2).await.is_none());
    }

    #[tokio::test]
    async fn test_export_import() {
        let graph = KnowledgeGraph::new();

        let e1 = graph.add_entity(Entity::new("E1", "Type")).await.unwrap();
        let e2 = graph.add_entity(Entity::new("E2", "Type")).await.unwrap();
        graph.relate(e1, "relates_to", e2).await.unwrap();

        let export = graph.export().await;

        assert_eq!(export.entities.len(), 2);
        assert_eq!(export.relationships.len(), 1);

        // Import into new graph
        let graph2 = KnowledgeGraph::new();
        let (entities, rels) = graph2.import(export).await.unwrap();

        assert_eq!(entities, 2);
        assert_eq!(rels, 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let graph = KnowledgeGraph::new();

        graph.add_entity(Entity::new("E1", "TypeA")).await.unwrap();
        graph.add_entity(Entity::new("E2", "TypeA")).await.unwrap();
        graph.add_entity(Entity::new("E3", "TypeB")).await.unwrap();

        let stats = graph.stats().await;

        assert_eq!(stats.entity_count, 3);
        assert_eq!(stats.entities_by_type.get("TypeA"), Some(&2));
        assert_eq!(stats.entities_by_type.get("TypeB"), Some(&1));
    }

    #[test]
    fn test_filter_operations() {
        let entity = Entity::new("Test", "Type")
            .with_property("name", "hello world")
            .with_property("count", 10i64);

        // Equals
        assert!(KnowledgeGraph::check_property_filter(
            &entity,
            &PropertyFilter::equals("name", "hello world")
        ));

        // Contains
        assert!(KnowledgeGraph::check_property_filter(
            &entity,
            &PropertyFilter::contains("name", "world")
        ));

        // Greater than
        assert!(KnowledgeGraph::check_property_filter(
            &entity,
            &PropertyFilter::greater_than("count", 5i64)
        ));

        // Less than
        assert!(KnowledgeGraph::check_property_filter(
            &entity,
            &PropertyFilter::less_than("count", 20i64)
        ));
    }
}
