import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { Dialog, DialogContent, DialogTitle, ConfirmDialog } from "./dialog";

describe("Dialog component", () => {
  it("renders when open is true", () => {
    render(
      <Dialog open={true} onOpenChange={() => {}}>
        <DialogContent>
          <DialogTitle>Test Dialog</DialogTitle>
        </DialogContent>
      </Dialog>
    );
    expect(screen.getByText("Test Dialog")).toBeInTheDocument();
  });

  it("does not render when open is false", () => {
    render(
      <Dialog open={false} onOpenChange={() => {}}>
        <DialogContent>
          <DialogTitle>Hidden Dialog</DialogTitle>
        </DialogContent>
      </Dialog>
    );
    expect(screen.queryByText("Hidden Dialog")).not.toBeInTheDocument();
  });

  it("enforces exact typed confirmation before enabling Confirm button", () => {
    const onConfirmMock = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        onOpenChange={() => {}}
        title="Delete Domain"
        description="Irreversible operation"
        confirmText="delete-example.com"
        onConfirm={onConfirmMock}
      />
    );

    const confirmBtn = screen.getByRole("button", { name: /^confirm$/i });
    expect(confirmBtn).toBeDisabled();

    const input = screen.getByPlaceholderText("delete-example.com");
    fireEvent.change(input, { target: { value: "wrong-text" } });
    expect(confirmBtn).toBeDisabled();

    fireEvent.change(input, { target: { value: "delete-example.com" } });
    expect(confirmBtn).toBeEnabled();

    fireEvent.click(confirmBtn);
    expect(onConfirmMock).toHaveBeenCalledTimes(1);
  });
});
