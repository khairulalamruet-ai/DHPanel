import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { LanguageSwitcher } from "./LanguageSwitcher";
import i18n from "./i18n";

describe("LanguageSwitcher component", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("bn");
  });

  it("renders with initial Bengali language indicator", () => {
    render(<LanguageSwitcher />);
    expect(screen.getByText("বাং")).toBeInTheDocument();
  });

  it("toggles between bn and en when clicked", async () => {
    render(<LanguageSwitcher />);
    const button = screen.getByRole("button");

    expect(i18n.language).toMatch(/^bn/);
    expect(screen.getByText("বাং")).toBeInTheDocument();

    // Click to switch to en
    fireEvent.click(button);
    expect(i18n.language).toMatch(/^en/);
    expect(screen.getByText("EN")).toBeInTheDocument();

    // Click to switch back to bn
    fireEvent.click(button);
    expect(i18n.language).toMatch(/^bn/);
    expect(screen.getByText("বাং")).toBeInTheDocument();
  });
});
