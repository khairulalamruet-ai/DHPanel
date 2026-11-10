import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { ToastProvider, useToast } from "./toast";
import { Button } from "./button";

function ToastTestComponent() {
  const { toast } = useToast();
  return (
    <Button
      onClick={() =>
        toast({
          title: "Test Toast",
          description: "This is a toast message",
          variant: "success",
        })
      }
    >
      Show Toast
    </Button>
  );
}

describe("Toast component", () => {
  it("renders toast when triggered", () => {
    render(
      <ToastProvider>
        <ToastTestComponent />
      </ToastProvider>
    );

    expect(screen.queryByText("Test Toast")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /show toast/i }));
    expect(screen.getByText("Test Toast")).toBeInTheDocument();
    expect(screen.getByText("This is a toast message")).toBeInTheDocument();
  });
});
