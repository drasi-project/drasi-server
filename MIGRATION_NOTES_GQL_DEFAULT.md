# Migration Note: Query Language Default Changed to GQL

## Summary
The default query language for Continuous Queries has been changed from `Cypher` to `GQL`.

## What This Means

### For New Queries
If you create a new query without specifying `queryLanguage`, it will now default to GQL:

```yaml
queries:
  - id: my-new-query
    query: "MATCH (n) RETURN n"  # Will be interpreted as GQL
    sources:
      - sourceId: my-source
```

### For Existing Queries
If you have existing queries that use Cypher (openCypher) syntax, you **must** now explicitly specify the language:

```yaml
queries:
  - id: my-existing-query
    query: "MATCH (n) RETURN n"
    queryLanguage: Cypher  # ← Add this line
    sources:
      - sourceId: my-source
```

## Migration Checklist

1. **Review all your query definitions** in configuration files
2. **Add `queryLanguage: Cypher`** to any queries using Cypher/openCypher syntax
3. **Leave queries using GQL syntax** as-is (or optionally add `queryLanguage: GQL` for clarity)
4. **Test your queries** to ensure they still work as expected

## Example Migration

### Before (with implicit Cypher default)
```yaml
queries:
  - id: active-users
    query: |
      MATCH (u:User)
      WHERE u.active = true
      RETURN u.id, u.name
    sources:
      - sourceId: users-db
```

### After (explicit language)
```yaml
queries:
  - id: active-users
    query: |
      MATCH (u:User)
      WHERE u.active = true
      RETURN u.id, u.name
    queryLanguage: Cypher  # ← Added for existing Cypher queries
    sources:
      - sourceId: users-db
```

## Why This Change?

GQL (Graph Query Language) is becoming the standard for graph queries, and this change aligns Drasi with industry standards. Cypher remains fully supported through the explicit `queryLanguage` setting.

## Questions?

- See [README.md](README.md) for full query configuration documentation
- Both Cypher and GQL are fully supported - this only changes the default
- No functionality is removed, only the default behavior changes
