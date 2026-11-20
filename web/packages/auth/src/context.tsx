import React, { createContext, useContext, useEffect, useState } from "react";
import { setCsrfToken, getCsrfToken, apiFetch, api } from "@builtpanel/api";
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
  bootstrapPath?: string;
}

export function SessionProvider({ children, bootstrapPath = "/api/v1/auth/me" }: SessionProviderProps) {
  const [data, setData] = useState<BootstrapData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetchSession = async () => {
    try {
      setIsLoading(true);
      setError(null);
      const res = await apiFetch<any>(bootstrapPath);
      const normalized: BootstrapData = res.user
        ? res
        : {
            user: {
              id: res.id,
              username: res.username,
              email: res.email,
              name: res.username,
              kind: res.kind,
            },
            roles: res.roles || [],
            permissions: res.permissions || [],
            modules: ["dashboard", "domains", "security"],
            features: ["all"],
            limits: {},
            branding: { companyName: "Built Panel" },
            locale: res.locale || "en",
            csrfToken: res.csrf_token,
            mfa_enabled: res.mfa_enabled,
          };
      setData(normalized);
      if (normalized.csrfToken || res.csrf_token) {
        setCsrfToken(normalized.csrfToken || res.csrf_token);
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
    if (data.roles.includes("super_admin") || data.permissions.includes("*")) return true;
    if (data.permissions.includes(perm)) return true;
    return data.permissions.some((p) => {
      if (p.endsWith(".*")) {
        const prefix = p.slice(0, -2);
        return perm === prefix || perm.startsWith(`${prefix}.`);
      }
      return false;
    });
  };

  const hasFeature = (feature: string): boolean => {
    if (!data) return false;
    if (data.features?.includes("all")) return true;
    return data.features?.includes(feature) ?? false;
  };

  const hasRole = (role: string): boolean => {
    if (!data) return false;
    return data.roles.includes(role);
  };

  const logout = async () => {
    try {
      await api.POST("/api/v1/auth/logout", {
        params: { header: { "X-CSRF-Token": getCsrfToken() || "" } },
      });
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
