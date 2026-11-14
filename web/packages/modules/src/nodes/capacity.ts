import type { NodeItem } from "./types";

/**
 * Calculates or returns the saturation percentage (0-100) of a node.
 * Fulfils DoD: "Admin UI-তে node-প্রতি পূর্ণতা (%) দেখা যায়।"
 */
export function computeNodeSaturation(node: NodeItem): number {
  if (typeof node.capacity?.saturation_pct === "number") {
    return Math.max(0, Math.min(100, Math.round(node.capacity.saturation_pct)));
  }

  // Fallback: calculate from capabilities and capacity if defined
  const cpuRatio = node.capacity?.oversell.cpu_ratio ?? 2.0;
  const ramRatio = node.capacity?.oversell.ram_ratio ?? 1.25;
  const diskRatio = node.capacity?.oversell.disk_ratio ?? 1.0;

  const totalCpuEffective = Math.max(1, (node.capabilities.cpu_cores || 1) * cpuRatio);
  const totalRamEffective = Math.max(1, (node.capabilities.ram_total_bytes || 1) * ramRatio);
  const totalDiskEffective = Math.max(1, (node.capabilities.disk_total_bytes || 1) * diskRatio);

  const allocatedCpu = node.capacity?.allocated_cpu_cores ?? Math.round((node.capabilities.cpu_usage_pct / 100) * (node.capabilities.cpu_cores || 1));
  const allocatedRam = node.capacity?.allocated_ram_bytes ?? node.capabilities.ram_used_bytes;
  const allocatedDisk = node.capacity?.allocated_disk_bytes ?? node.capabilities.disk_used_bytes;

  const cpuPct = (allocatedCpu / totalCpuEffective) * 100;
  const ramPct = (allocatedRam / totalRamEffective) * 100;
  const diskPct = (allocatedDisk / totalDiskEffective) * 100;

  const maxSaturation = Math.max(cpuPct, ramPct, diskPct);
  return Math.max(0, Math.min(100, Math.round(maxSaturation)));
}

/**
 * Returns color classes based on saturation threshold.
 */
export function getSaturationColor(pct: number) {
  if (pct >= 90) {
    return {
      text: "text-red-600 dark:text-red-400",
      bg: "bg-red-500/10",
      border: "border-red-500/30",
      bar: "bg-red-500",
      badgeVariant: "destructive" as const,
    };
  }
  if (pct >= 75) {
    return {
      text: "text-amber-600 dark:text-amber-400",
      bg: "bg-amber-500/10",
      border: "border-amber-500/30",
      bar: "bg-amber-500",
      badgeVariant: "outline" as const,
    };
  }
  return {
    text: "text-emerald-600 dark:text-emerald-400",
    bg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
    bar: "bg-emerald-500",
    badgeVariant: "success" as const,
  };
}
