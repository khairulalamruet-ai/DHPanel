export interface UserProfile {
  id: string;
  username: string;
  email: string;
  name: string;
}

export interface BootstrapData {
  user: UserProfile;
  roles: string[];
  permissions: string[];
  modules: string[];
  features: string[];
  limits: Record<string, number>;
  branding?: {
    companyName?: string;
    logoUrl?: string;
  };
  locale?: string;
  csrfToken?: string;
}
