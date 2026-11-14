export type SearchCategory = "pages" | "domains" | "users" | "actions";

export interface SearchResultItem {
  id: string;
  title: string;
  subtitle?: string;
  category: SearchCategory;
  href?: string;
  badge?: string;
  onSelect?: () => void;
}

export interface SearchGroup {
  category: SearchCategory;
  label: string;
  items: SearchResultItem[];
}
