import React, { useRef, useState, useEffect, useMemo, useCallback } from "react";

export interface AnyLogEntry {
  id: string;
  lineNumber?: number;
  timestamp: string;
  domain?: string;
  service?: string;
  level?: "info" | "warn" | "error" | "debug" | "fatal";
  status_code?: number;
  method?: string;
  path?: string;
  ip?: string;
  component?: string;
  message: string;
  raw: string;
}

interface VirtualizedLogListProps {
  entries: AnyLogEntry[];
  itemHeight?: number;
  searchQuery?: string;
  autoScrollBottom?: boolean;
  onScrollStateChange?: (isAtBottom: boolean) => void;
}

export function VirtualizedLogList({
  entries,
  itemHeight = 26,
  searchQuery = "",
  autoScrollBottom = false,
  onScrollStateChange,
}: VirtualizedLogListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(550);

  // Measure container height on mount and resize
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    setContainerHeight(el.clientHeight || 550);

    if (typeof ResizeObserver !== "undefined") {
      const observer = new ResizeObserver((items) => {
        if (items[0]) {
          setContainerHeight(items[0].contentRect.height);
        }
      });

      observer.observe(el);
      return () => observer.disconnect();
    }
  }, []);

  // Handle scroll events with RAF throttling for optimal performance
  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;

    setScrollTop(el.scrollTop);

    const isAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    if (onScrollStateChange) {
      onScrollStateChange(isAtBottom);
    }
  }, [onScrollStateChange]);

  // Auto scroll to bottom when new entries arrive and autoScrollBottom is enabled
  useEffect(() => {
    if (autoScrollBottom && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [entries.length, autoScrollBottom]);

  // Filter entries based on search query
  const filteredEntries = useMemo(() => {
    if (!searchQuery.trim()) return entries;
    const q = searchQuery.toLowerCase();
    return entries.filter(
      (e) =>
        e.raw.toLowerCase().includes(q) ||
        (e.path && e.path.toLowerCase().includes(q)) ||
        (e.ip && e.ip.includes(q)) ||
        (e.message && e.message.toLowerCase().includes(q))
    );
  }, [entries, searchQuery]);

  const totalCount = filteredEntries.length;
  const totalHeight = totalCount * itemHeight;

  // Virtualization window slice calculation with overscan
  const overscan = 15;
  const startIndex = Math.max(0, Math.floor(scrollTop / itemHeight) - overscan);
  const endIndex = Math.min(
    totalCount,
    Math.ceil((scrollTop + containerHeight) / itemHeight) + overscan
  );

  const visibleEntries = useMemo(() => {
    return filteredEntries.slice(startIndex, endIndex).map((entry, idx) => ({
      entry,
      actualIndex: startIndex + idx,
    }));
  }, [filteredEntries, startIndex, endIndex]);

  const formatLineStatusColor = (entry: AnyLogEntry) => {
    if (entry.level === "fatal" || entry.level === "error" || (entry.status_code && entry.status_code >= 500)) {
      return "text-red-400 bg-red-950/20";
    }
    if (entry.level === "warn" || (entry.status_code && entry.status_code >= 400)) {
      return "text-amber-400 bg-amber-950/20";
    }
    if (entry.status_code && entry.status_code >= 300) {
      return "text-cyan-400";
    }
    if (entry.status_code && entry.status_code >= 200) {
      return "text-green-400";
    }
    return "text-zinc-300";
  };

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className="w-full h-full min-h-[480px] max-h-[700px] overflow-y-auto overflow-x-auto rounded-lg bg-zinc-950 text-zinc-200 font-mono text-xs select-text border border-zinc-800"
      data-testid="virtualized-log-container"
    >
      {totalCount === 0 ? (
        <div className="h-full min-h-[400px] flex items-center justify-center text-zinc-500 font-sans text-sm">
          No log entries matching criteria
        </div>
      ) : (
        <div
          style={{ height: `${totalHeight}px`, position: "relative" }}
          className="w-full"
        >
          <div
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              transform: `translateY(${startIndex * itemHeight}px)`,
            }}
          >
            {visibleEntries.map(({ entry, actualIndex }) => {
              const statusColor = formatLineStatusColor(entry);

              return (
                <div
                  key={entry.id || actualIndex}
                  data-testid={`virtualized-log-line-${actualIndex}`}
                  style={{ height: `${itemHeight}px` }}
                  className={`flex items-center px-3 hover:bg-zinc-900/80 transition-colors border-b border-zinc-900/50 leading-none whitespace-nowrap ${statusColor}`}
                >
                  {/* Line Number */}
                  <span className="w-12 text-zinc-600 select-none text-[11px] text-right pr-3 shrink-0">
                    {entry.lineNumber || actualIndex + 1}
                  </span>

                  {/* Timestamp */}
                  <span className="text-zinc-500 text-[11px] pr-3 shrink-0">
                    {entry.timestamp ? entry.timestamp.slice(11, 19) : "00:00:00"}
                  </span>

                  {/* Domain Tag */}
                  {entry.domain && (
                    <span className="text-zinc-400 text-[11px] px-1.5 py-0.5 rounded bg-zinc-800/60 mr-2 shrink-0">
                      {entry.domain}
                    </span>
                  )}

                  {/* Level / Status Code Badge */}
                  {entry.status_code != null && (
                    <span
                      className={`font-bold px-1.5 py-0.5 rounded text-[11px] mr-2 shrink-0 ${
                        entry.status_code >= 500
                          ? "bg-red-500/20 text-red-400"
                          : entry.status_code >= 400
                          ? "bg-amber-500/20 text-amber-400"
                          : entry.status_code >= 300
                          ? "bg-blue-500/20 text-blue-400"
                          : "bg-green-500/20 text-green-400"
                      }`}
                    >
                      {entry.status_code}
                    </span>
                  )}

                  {entry.level && (
                    <span
                      className={`font-semibold uppercase text-[10px] px-1.5 py-0.5 rounded mr-2 shrink-0 ${
                        entry.level === "fatal"
                          ? "bg-rose-600/30 text-rose-300 font-extrabold"
                          : entry.level === "error"
                          ? "bg-red-500/20 text-red-400"
                          : entry.level === "warn"
                          ? "bg-amber-500/20 text-amber-400"
                          : "bg-zinc-800 text-zinc-400"
                      }`}
                    >
                      {entry.level}
                    </span>
                  )}

                  {/* Component tag if present */}
                  {entry.component && (
                    <span className="text-zinc-400 text-[11px] px-1.5 py-0.5 rounded bg-zinc-800/80 mr-2 shrink-0">
                      [{entry.component}]
                    </span>
                  )}

                  {/* Method & Path if access log */}
                  {entry.method && (
                    <span className="font-semibold text-zinc-300 mr-1 shrink-0">
                      {entry.method}
                    </span>
                  )}

                  {entry.path && (
                    <span className="text-zinc-200 mr-2 shrink-0 font-medium">
                      {entry.path}
                    </span>
                  )}

                  {/* Message / Raw */}
                  <span className="truncate text-zinc-300">
                    {entry.message || entry.raw}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
