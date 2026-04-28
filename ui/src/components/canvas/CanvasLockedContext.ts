import { createContext, useContext } from "react";

export const CanvasLockedContext = createContext(false);

export function useCanvasLocked(): boolean {
  return useContext(CanvasLockedContext);
}
