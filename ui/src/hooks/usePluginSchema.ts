import { useState, useEffect } from 'react';

interface PluginSchema {
  kind: string;
  category: string;
  configVersion: string;
  schemaName: string;
  schema: Record<string, any>;
}

export function usePluginSchema(category: string, kind: string) {
  const [schema, setSchema] = useState<PluginSchema | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!category || !kind) {
      setLoading(false);
      return;
    }

    fetch(`/api/v1/plugins/kinds/${category}/${kind}/schema`)
      .then(res => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then(data => {
        setSchema(data);
        setLoading(false);
      })
      .catch(err => {
        setError(err.message);
        setLoading(false);
      });
  }, [category, kind]);

  return { schema, loading, error };
}
