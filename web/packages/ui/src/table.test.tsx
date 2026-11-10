import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import React from "react";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "./table";

describe("Table component", () => {
  it("renders tabular structure with headers and rows", () => {
    render(
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Domain</TableHead>
            <TableHead>IP</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow>
            <TableCell>example.com</TableCell>
            <TableCell>127.0.0.1</TableCell>
          </TableRow>
        </TableBody>
      </Table>
    );

    expect(screen.getByText("Domain")).toBeInTheDocument();
    expect(screen.getByText("example.com")).toBeInTheDocument();
    expect(screen.getByText("127.0.0.1")).toBeInTheDocument();
  });
});
