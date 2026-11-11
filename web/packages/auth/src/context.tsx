import React, { createContext, useContext, useEffect, useState } from "react";
import { setCsrfToken, apiFetch, api } from "@dhpanel/api";
import { BootstrapData } from "./types";

interface SessionContextType {
  data: BootstrapData | null;
  isLoading: boolean;
  error: Error | null;
  hasPermission: (perm: string) => boolean;
  hasFeature: (feature: string) => boolean;
  hasRole: (role: string) => boolean;
  refetch: () => Promise<void>;
  logout: () => Promise<void>;
  setSessionData: (data: BootstrapData | null) => void;
}

const SessionContext = createContext<SessionContextType | undefined>(undefined);

export interface SessionProviderProps {
  children: React.ReactNode;
  bootstrapPath: "/api/v1/admin/bootstrap" | "/api/v1/client/bootstrap";
}

export function SessionProvider({ children, bootstrapPath }: SessionProviderProps) {
  const [data, setData] = useState<BootstrapData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetchSession = async () => {
    try {
      setIsLoading(true);
      setError(null);
      const res = await apiFetch<BootstrapData>(bootstrapPath);
      setData(res);
      if (res.csrfToken) {
        setCsrfToken(res.csrfToken);
      }
    } catch (err: any) {
      setError(err instanceof Error ? err : new Error(String(err)));
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchSession();
  }, [bootstrapPath]);

  const hasPermission = (perm: string): boolean => {
    if (!data) return false;
    if (data.roles.includes("super_admin")) return true;
    return data.permissions.includes(perm);
  };

  const hasFeature = (feature: string): boolean => {
    if (!data) return false;
    if (data.features.includes("all")) return true;
    return data.features.includes(feature);
  };

  const hasRole = (role: string): boolean => {
    if (!data) return false;
    return data.roles.includes(role);
  };

  const logout = async () => {
    try {
      await api.POST("/api/v1/public/logout");
    } catch {
      // Ignore network errors on logout
    } finally {
      setData(null);
      setCsrfToken(null);
    }
  };

  return (
    <SessionContext.Provider
      value={{
        data,
        isLoading,
        error,
        hasPermission,
        hasFeature,
        hasRole,
        refetch: fetchSession,
        logout,
        setSessionData: setData,
      }}
    >
      {children}
    </SessionContext.Provider>
  );
}

export function useSession() {
  const context = useContext(SessionContext);
  if (!context) {
    throw new Error("useSession must be used within a SessionProvider");
  }
  return context;
}
