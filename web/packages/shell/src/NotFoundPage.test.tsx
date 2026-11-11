import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { NotFoundPage } from "./NotFoundPage";
import "../../i18n/src/i18n";

describe("NotFoundPage component", () => {
  it("renders 404 title and description", () => {
    render(<NotFoundPage />);
    expect(screen.getByText(/৪০৪ — পেজ পাওয়া যায়নি|404 - Page Not Found/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ড্যাশবোর্ডে ফিরে যান|Back to Dashboard/i })).toBeInTheDocument();
  });

  it("calls onNavigateHome when return button is clicked", () => {
    const onHomeMock = vi.fn();
    render(<NotFoundPage onNavigateHome={onHomeMock} />);
    const btn = screen.getByRole("button", { name: /ড্যাশবোর্ডে ফিরে যান|Back to Dashboard/i });
    fireEvent.click(btn);
    expect(onHomeMock).toHaveBeenCalledTimes(1);
  });
});
