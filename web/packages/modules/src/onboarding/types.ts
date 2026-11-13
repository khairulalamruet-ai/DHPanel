export type AdminQuickstartStepId =
  | "add_node"
  | "create_plan"
  | "configure_dns"
  | "setup_smtp";

export interface AdminQuickstartStep {
  id: AdminQuickstartStepId;
  titleKey: string;
  defaultTitle: string;
  descKey: string;
  defaultDesc: string;
  actionUrl: string;
  actionKey: string;
  defaultAction: string;
  completed: boolean;
}

export interface AdminQuickstartState {
  dismissed: boolean;
  steps: AdminQuickstartStep[];
}

export interface ClientTourStep {
  id: string;
  titleKey: string;
  defaultTitle: string;
  descKey: string;
  defaultDesc: string;
  actionUrl?: string;
  actionKey?: string;
  defaultAction?: string;
}

export interface ClientTourState {
  dismissed: boolean;
  currentStepIndex: number;
  steps: ClientTourStep[];
}
