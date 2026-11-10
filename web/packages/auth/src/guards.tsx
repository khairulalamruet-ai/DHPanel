import React from "react";
import { useSession } from "./context";

export interface CanProps {
  permission: string;
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

export function Can({ permission, children, fallback = null }: CanProps) {
  const { hasPermission, isLoading } = useSession();
  if (isLoading) return null;
  return hasPermission(permission) ? <>{children}</> : <>{fallback}</>;
}

export interface FeatureGateProps {
  feature: string;
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

export function FeatureGate({ feature, children, fallback = null }: FeatureGateProps) {
  const { hasFeature, isLoading } = useSession();
  if (isLoading) return null;
  return hasFeature(feature) ? <>{children}</> : <>{fallback}</>;
}

export interface RoleGateProps {
  role: string;
  children: React.ReactNode;
  fallback?: React.ReactNode;
}

export function RoleGate({ role, children, fallback = null }: RoleGateProps) {
  const { hasRole, isLoading } = useSession();
  if (isLoading) return null;
  return hasRole(role) ? <>{children}</> : <>{fallback}</>;
}
