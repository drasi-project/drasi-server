export function requireInstanceId(instanceId: string | undefined): string {
  if (!instanceId) throw new Error("Select an instance before operating on components.");
  return instanceId;
}

export function instancePath(instanceId: string, ...segments: string[]): string {
  const ids = [requireInstanceId(instanceId), ...segments];
  if (ids.some((id) => !id || id === "." || id === "..")) {
    throw new Error("Invalid instance or component identifier.");
  }
  return `/instances/${ids.map(encodeURIComponent).join("/")}`;
}
