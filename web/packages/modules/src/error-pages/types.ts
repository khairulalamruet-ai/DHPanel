export type PageTemplateType =
  | "suspended_domain"
  | "suspended_user"
  | "error_404"
  | "error_502";

export interface PageTemplateVariable {
  name: string;
  description: string;
  sample: string;
}

export interface PageTemplateItem {
  id: string;
  type: PageTemplateType;
  title: string;
  description: string;
  statusCode: number;
  content: string;
  defaultContent: string;
  updatedAt: string;
  isCustomized: boolean;
  variables: PageTemplateVariable[];
}

export interface PageTemplateListResponse {
  items: PageTemplateItem[];
  total: number;
}

export interface UpdatePageTemplateRequest {
  content: string;
}

export interface ResetPageTemplateResponse {
  success: boolean;
  item: PageTemplateItem;
}
