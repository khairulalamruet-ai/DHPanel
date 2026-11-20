import React from "react";
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { ToastProvider } from "@builtpanel/ui";
import { resetMockSsl } from "@builtpanel/api";
import { SslPage } from "./SslPage";

function renderWithProviders(ui: React.ReactElement) {
  return render(<ToastProvider>{ui}</ToastProvider>);
}

describe("SslPage (T05.19)", () => {
  beforeEach(() => {
    resetMockSsl();
  });

  it("renders page title, metric cards, and initial seeded domain certificates", async () => {
    renderWithProviders(<SslPage />);

    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();

    // Verify initial seeded certificates loaded
    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "alpha-hosting.com" })).toBeInTheDocument();
    });

    expect(screen.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "beta-app.io" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "dev.internal-net.org" })).toBeInTheDocument();

    // Check metric cards
    expect(screen.getByText("4")).toBeInTheDocument(); // total
  });

  it("filters certificates by status pill and search keyword", async () => {
    renderWithProviders(<SslPage />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "alpha-hosting.com" })).toBeInTheDocument();
    });

    // Filter by expired
    const expiredFilterBtn = screen.getByTestId("btn-filter-ssl-expired");
    fireEvent.click(expiredFilterBtn);

    expect(screen.getByRole("heading", { level: 3, name: "beta-app.io" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 3, name: "alpha-hosting.com" })).not.toBeInTheDocument();

    // Reset to all
    const allFilterBtn = screen.getByTestId("btn-filter-ssl-ALL");
    fireEvent.click(allFilterBtn);

    // Search by keyword
    const searchInput = screen.getByTestId("input-search-ssl");
    fireEvent.change(searchInput, { target: { value: "blog" } });

    expect(screen.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 3, name: "beta-app.io" })).not.toBeInTheDocument();
  });

  it("re-issues / renews SSL certificate via Let's Encrypt (DoD)", async () => {
    renderWithProviders(<SslPage />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com" })).toBeInTheDocument();
    });

    // Find the reissue button for blog.alpha-hosting.com (dom_02)
    const reissueBtn = screen.getByTestId("btn-reissue-ssl-dom_02");
    fireEvent.click(reissueBtn);

    // After reissuance, days remaining should be 90
    await waitFor(() => {
      expect(screen.getByText(/(90|৯০)\s*(days left|দিন বাকি)/)).toBeInTheDocument();
    });
  });

  it("uploads and installs custom commercial SSL certificate (DoD)", async () => {
    renderWithProviders(<SslPage />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "beta-app.io" })).toBeInTheDocument();
    });

    // Open Custom Cert dialog for beta-app.io (dom_03)
    const customBtn = screen.getByTestId("btn-upload-custom-ssl-dom_03");
    fireEvent.click(customBtn);

    await waitFor(() => {
      expect(screen.getByTestId("dialog-custom-ssl")).toBeInTheDocument();
    });

    // Enter PEM Cert and Private Key
    const certInput = screen.getByTestId("textarea-ssl-cert-pem");
    const keyInput = screen.getByTestId("textarea-ssl-key-pem");

    fireEvent.change(certInput, {
      target: {
        value: `-----BEGIN CERTIFICATE-----
MIIDXTCCAkWgAwIBAgIJAP999EXAMPLE
-----END CERTIFICATE-----`,
      },
    });

    fireEvent.change(keyInput, {
      target: {
        value: `-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAz999EXAMPLE
-----END RSA PRIVATE KEY-----`,
      },
    });

    // Submit form
    const submitBtn = screen.getByTestId("btn-submit-custom-ssl");
    fireEvent.click(submitBtn);

    // Modal closes and status changes to active with custom issuer
    await waitFor(() => {
      expect(screen.queryByTestId("dialog-custom-ssl")).not.toBeInTheDocument();
    });

    await waitFor(() => {
      expect(screen.getByText("Custom Uploaded CA")).toBeInTheDocument();
    });
  });

  it("configures HTTPS redirection and HSTS security headers", async () => {
    renderWithProviders(<SslPage />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "alpha-hosting.com" })).toBeInTheDocument();
    });

    // Open settings dialog for alpha-hosting.com (dom_01)
    const settingsBtn = screen.getByTestId("btn-settings-ssl-dom_01");
    fireEvent.click(settingsBtn);

    await waitFor(() => {
      expect(screen.getByTestId("dialog-ssl-settings")).toBeInTheDocument();
    });

    // Check checkboxes
    const forceHttpsCheckbox = screen.getByTestId("checkbox-force-https");
    const hstsCheckbox = screen.getByTestId("checkbox-hsts");

    fireEvent.click(forceHttpsCheckbox);
    fireEvent.click(hstsCheckbox);

    // Save settings
    const saveBtn = screen.getByTestId("btn-save-ssl-settings");
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(screen.queryByTestId("dialog-ssl-settings")).not.toBeInTheDocument();
    });
  });

  it("revokes SSL certificate through confirmation dialog", async () => {
    renderWithProviders(<SslPage />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 3, name: "alpha-hosting.com" })).toBeInTheDocument();
    });

    // Open revoke dialog for alpha-hosting.com (dom_01)
    const revokeBtn = screen.getByTestId("btn-revoke-ssl-dom_01");
    fireEvent.click(revokeBtn);

    // Confirm revoke
    const confirmBtn = screen.getByTestId("btn-confirm-revoke-ssl");
    fireEvent.click(confirmBtn);

    // Certificate should now have "none" status
    await waitFor(() => {
      expect(screen.getByText(/No Certificate|কোনো সার্টিফিকেট নেই/)).toBeInTheDocument();
    });
  });
});
