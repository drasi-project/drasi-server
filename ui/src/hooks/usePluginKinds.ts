import { useState, useEffect } from "react";

interface PluginKind {
  kind: string;
  configVersion: string;
  configSchemaJson: string;
  configSchemaName: string;
  pluginId: string;
}

export interface PluginKinds {
  sources: PluginKind[];
  reactions: PluginKind[];
  bootstrappers: PluginKind[];
}

export function usePluginKinds() {
  const [kinds, setKinds] = useState<PluginKinds | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/v1/plugins/kinds")
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then((data) => {
        setKinds(data);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  }, []);

  return { kinds, loading, error };
}
