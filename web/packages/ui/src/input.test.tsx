import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import React from "react";
import { Input } from "./input";

describe("Input component", () => {
  it("renders with label and helper text", () => {
    render(<Input label="Username" helperText="Your unique login name" />);
    expect(screen.getByLabelText(/username/i)).toBeInTheDocument();
    expect(screen.getByText(/your unique login name/i)).toBeInTheDocument();
  });

  it("displays error message and applies error class", () => {
    render(<Input label="Email" error="Invalid email address" />);
    expect(screen.getByText(/invalid email address/i)).toBeInTheDocument();
    const input = screen.getByLabelText(/email/i);
    expect(input).toHaveClass("border-destructive");
  });
});
