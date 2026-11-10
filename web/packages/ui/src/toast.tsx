import React, { createContext, useContext, useState, useCallback } from "react";
import { cn } from "./lib/utils";
import { X, CheckCircle, AlertCircle, Info } from "lucide-react";

export type ToastVariant = "default" | "destructive" | "success" | "info";

export interface ToastItem {
  id: string;
  title?: string;
  description?: string;
  variant?: ToastVariant;
  duration?: number;
}

interface ToastContextType {
  toasts: ToastItem[];
  toast: (props: Omit<ToastItem, "id">) => void;
  dismiss: (id: string) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const toast = useCallback(
    ({ title, description, variant = "default", duration = 4000 }: Omit<ToastItem, "id">) => {
      const id = Math.random().toString(36).slice(2, 9);
      setToasts((prev) => [...prev, { id, title, description, variant, duration }]);

      if (duration > 0) {
        setTimeout(() => {
          dismiss(id);
        }, duration);
      }
    },
    [dismiss]
  );

  return (
    <ToastContext.Provider value={{ toasts, toast, dismiss }}>
      {children}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return context;
}

function ToastViewport({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: string) => void;
}) {
  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-0 right-0 z-50 flex max-h-screen w-full flex-col-reverse p-4 sm:bottom-0 sm:right-0 sm:top-auto sm:flex-col md:max-w-[420px] gap-2">
      {toasts.map((item) => (
        <div
          key={item.id}
          className={cn(
            "pointer-events-auto relative flex w-full items-center justify-between space-x-4 overflow-hidden rounded-md border p-4 pr-8 shadow-lg transition-all",
            item.variant === "destructive" && "border-destructive bg-destructive text-destructive-foreground",
            item.variant === "success" && "border-emerald-600 bg-emerald-600 text-white",
            item.variant === "info" && "border-blue-600 bg-blue-600 text-white",
            (!item.variant || item.variant === "default") && "border-border bg-card text-card-foreground"
          )}
        >
          <div className="flex items-start gap-3">
            {item.variant === "success" && <CheckCircle className="h-5 w-5 shrink-0" />}
            {item.variant === "destructive" && <AlertCircle className="h-5 w-5 shrink-0" />}
            {item.variant === "info" && <Info className="h-5 w-5 shrink-0" />}
            <div className="grid gap-1">
              {item.title && <div className="text-sm font-semibold">{item.title}</div>}
              {item.description && <div className="text-xs opacity-90">{item.description}</div>}
            </div>
          </div>
          <button
            onClick={() => onDismiss(item.id)}
            className="absolute right-2 top-2 rounded-md p-1 opacity-70 hover:opacity-100 focus:outline-none"
            aria-label="Dismiss toast"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
