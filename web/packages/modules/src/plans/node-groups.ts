export interface NodeGroup {
  id: string;
  name: string;
  description: string;
  region: string;
  tags: string[];
  nodes_count: number;
  is_default?: boolean;
}

export const PRESET_NODE_GROUPS: NodeGroup[] = [
  {
    id: "ng_global",
    name: "Global Cluster (Default)",
    description: "Default multi-region pool matching all standard tenant nodes.",
    region: "global",
    tags: ["general", "standard"],
    nodes_count: 4,
    is_default: true,
  },
  {
    id: "ng_us_east",
    name: "US-East General Pool",
    description: "North American low-latency cluster pool on NVMe storage.",
    region: "us-east-1",
    tags: ["us-east", "nvme", "ssd"],
    nodes_count: 2,
  },
  {
    id: "ng_sgp_highmem",
    name: "Singapore High-RAM Pool",
    description: "Asia-Pacific memory-optimized nodes for enterprise workloads.",
    region: "ap-southeast-1",
    tags: ["apac", "highmem", "high-cpu"],
    nodes_count: 1,
  },
  {
    id: "ng_fra_isolated",
    name: "Frankfurt EU-West Group",
    description: "GDPR-compliant isolated cluster strictly hosted in Frankfurt EU-West.",
    region: "eu-central-1",
    tags: ["eu", "gdpr", "dedicated"],
    nodes_count: 1,
  },
];

export function getNodeGroupById(id?: string | null): NodeGroup {
  if (!id) return PRESET_NODE_GROUPS[0];
  const found = PRESET_NODE_GROUPS.find((g) => g.id === id);
  return found || PRESET_NODE_GROUPS[0];
}
