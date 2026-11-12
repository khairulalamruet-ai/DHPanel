import React from "react";
import { describe, it, expect } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import { ToastProvider } from "@dhpanel/ui";
import { UserCreateWizardPage } from "./UserCreateWizardPage";

function renderWithProviders(ui: React.ReactElement) {
  return render(<ToastProvider>{ui}</ToastProvider>);
}

describe("UserCreateWizardPage (T04.16)", () => {
  it("renders wizard header and initial step 1 (Account Credentials)", () => {
    renderWithProviders(<UserCreateWizardPage />);

    // Header title
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();

    // Step 1 title
    expect(
      screen.getByText(/Account Credentials|অ্যাকাউন্ট লগইন তথ্য/i)
    ).toBeInTheDocument();

    // Fields
    expect(screen.getByLabelText(/Username|ইউজারনেম/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Email|ইমেইল/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Password|পাসওয়ার্ড/i)).toBeInTheDocument();
  });

  it("validates step 1 inputs and blocks navigation on invalid data", () => {
    renderWithProviders(<UserCreateWizardPage />);

    const nextBtn = screen.getByRole("button", { name: /Next Step|পরবর্তী ধাপ/i });
    fireEvent.click(nextBtn);

    // Validation errors appear
    expect(screen.getByText(/Username is required|ইউজারনেম আবশ্যক/i)).toBeInTheDocument();

    // Test reserved username
    const usernameInput = screen.getByLabelText(/Username|ইউজারনেম/i);
    fireEvent.change(usernameInput, { target: { value: "root" } });
    fireEvent.click(nextBtn);
    expect(screen.getByText(/reserved by the system|সিস্টেম দ্বারা সংরক্ষিত/i)).toBeInTheDocument();
  });

  it("generates random password via button click", () => {
    renderWithProviders(<UserCreateWizardPage />);

    const genBtn = screen.getByRole("button", {
      name: /Generate Strong Password|শক্তিশালী পাসওয়ার্ড তৈরি করুন/i,
    });
    fireEvent.click(genBtn);

    const passwordInput = screen.getByLabelText(/Password|পাসওয়ার্ড/i) as HTMLInputElement;
    expect(passwordInput.value.length).toBeGreaterThanOrEqual(16);
  });

  it("DoD: completes full wizard flow from account to provisioning and displays container running", async () => {
    renderWithProviders(<UserCreateWizardPage />);

    // Step 1: Fill Account credentials
    const usernameInput = screen.getByLabelText(/Username|ইউজারনেম/i);
    const emailInput = screen.getByLabelText(/Email|ইমেইল/i);
    const passwordInput = screen.getByLabelText(/Password|পাসওয়ার্ড/i);

    fireEvent.change(usernameInput, { target: { value: "demouser" } });
    fireEvent.change(emailInput, { target: { value: "demouser@example.com" } });
    fireEvent.change(passwordInput, { target: { value: "SuperSecretPass123!" } });

    // Click Next -> Step 2 (Plan)
    const nextBtn = screen.getByRole("button", { name: /Next Step|পরবর্তী ধাপ/i });
    fireEvent.click(nextBtn);

    await waitFor(() => {
      expect(screen.getByText(/Select Hosting Plan|হোস্টিং প্যাকেজ নির্বাচন/i)).toBeInTheDocument();
    });

    // Step 2: Select Pro Developer Plan
    const proPlan = screen.getByText("Pro Developer");
    fireEvent.click(proPlan);

    // Click Next -> Step 3 (Node)
    fireEvent.click(screen.getByRole("button", { name: /Next Step|পরবর্তী ধাপ/i }));

    await waitFor(() => {
      expect(screen.getByText(/Cluster Node Assignment|ক্লাস্টার নোড নির্ধারণ/i)).toBeInTheDocument();
    });

    // Step 3: Default is Auto-Select, proceed Next -> Step 4 (Domain)
    fireEvent.click(screen.getByRole("button", { name: /Next Step|পরবর্তী ধাপ/i }));

    await waitFor(() => {
      expect(
        screen.getByRole("heading", { level: 3, name: /Primary Domain|প্রাথমিক ডোমেইন/i })
      ).toBeInTheDocument();
    });

    // Step 4: Add primary domain
    const domainInput = screen.getByPlaceholderText("mywebsite.com");
    fireEvent.change(domainInput, { target: { value: "demowebsite.com" } });

    // Click Next -> Step 5 (Review & Confirm)
    fireEvent.click(screen.getByRole("button", { name: /Next Step|পরবর্তী ধাপ/i }));

    await waitFor(() => {
      expect(
        screen.getByRole("heading", {
          level: 3,
          name: /Review & Confirm Provisioning|পর্যালোচনা ও প্রভিশনিং নিশ্চিতকরণ/i,
        })
      ).toBeInTheDocument();
    });

    // Verify summary values
    expect(screen.getByText("demouser")).toBeInTheDocument();
    expect(screen.getByText("demowebsite.com")).toBeInTheDocument();

    // Submit: Create User & Launch Container
    const confirmBtn = screen.getByRole("button", {
      name: /Create User & Launch Container|অ্যাকাউন্ট তৈরি ও কন্টেইনার চালু করুন/i,
    });
    fireEvent.click(confirmBtn);

    // Operation progress starts and finishes
    // DoD: container চলছে (container running indicator)
    await waitFor(
      () => {
        expect(screen.getByText("container running")).toBeInTheDocument();
      },
      { timeout: 5000 }
    );

    // Verify container name displayed
    expect(screen.getByText("dh-tenant-demouser")).toBeInTheDocument();
  });
});
