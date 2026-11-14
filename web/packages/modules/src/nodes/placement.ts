import type { NodeOption } from "../user-wizard/types";

export type PlacementPolicy = "least-loaded" | "round-robin" | "region-affinity" | "tag-affinity";

export interface PlacementResult {
  selectedNode: NodeOption | null;
  eligibleNodes: NodeOption[];
  excludedNodes: NodeOption[];
  reason: string;
}

/**
 * Selects an optimal node for tenant placement based on the active placement policy.
 * Strictly filters out any node with status === "draining" or status === "offline".
 * Fulfils T16.02 DoD: "Drain-এ থাকা node-এ নতুন tenant যায় না।"
 */
export function selectPlacementNode(
  nodes: NodeOption[],
  policy: PlacementPolicy = "least-loaded",
  requestedRegion?: string
): PlacementResult {
  // Exclude draining and offline nodes
  const excludedNodes = nodes.filter(
    (n) => n.status === "draining" || n.status === "offline"
  );
  const eligibleNodes = nodes.filter(
    (n) => n.status !== "draining" && n.status !== "offline"
  );

  if (eligibleNodes.length === 0) {
    return {
      selectedNode: null,
      eligibleNodes: [],
      excludedNodes,
      reason: "No eligible active nodes available in cluster. All nodes are either draining or offline.",
    };
  }

  if (policy === "region-affinity" && requestedRegion) {
    const matched = eligibleNodes.find((n) =>
      n.region.toLowerCase().includes(requestedRegion.toLowerCase())
    );
    if (matched) {
      return {
        selectedNode: matched,
        eligibleNodes,
        excludedNodes,
        reason: `Matched tenant region: ${matched.region}`,
      };
    }
  }

  if (policy === "least-loaded") {
    // Sort by lowest combined CPU & RAM usage
    const sorted = [...eligibleNodes].sort((a, b) => {
      const loadA = a.cpu_usage_pct + a.memory_usage_pct;
      const loadB = b.cpu_usage_pct + b.memory_usage_pct;
      return loadA - loadB;
    });
    return {
      selectedNode: sorted[0],
      eligibleNodes,
      excludedNodes,
      reason: `Least loaded node selected (${sorted[0].cpu_usage_pct}% CPU, ${sorted[0].memory_usage_pct}% RAM)`,
    };
  }

  // Default: round-robin / first available
  return {
    selectedNode: eligibleNodes[0],
    eligibleNodes,
    excludedNodes,
    reason: `Round-robin assignment to ${eligibleNodes[0].name}`,
  };
}
