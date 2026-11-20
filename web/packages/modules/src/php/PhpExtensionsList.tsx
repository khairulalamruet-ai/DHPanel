import React from "react";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, Badge } from "@builtpanel/ui";
import { Blocks, CheckCircle2 } from "lucide-react";

interface PhpExtensionsListProps {
  extensions: string[];
}

export function PhpExtensionsList({ extensions }: PhpExtensionsListProps) {
  return (
    <Card className="border" data-testid="php-extensions-card">
      <CardHeader className="pb-3">
        <CardTitle className="text-base flex items-center gap-2">
          <Blocks className="h-5 w-5 text-primary" />
          Compiled PHP Modules & Extensions
        </CardTitle>
        <CardDescription className="text-xs">
          Pre-installed and enabled PHP runtime extensions on this server node.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap gap-2">
          {extensions.map((ext) => (
            <Badge
              key={ext}
              variant="outline"
              className="text-xs font-mono py-1 px-2.5 gap-1.5 bg-muted/40 border-border"
              data-testid={`badge-php-ext-${ext}`}
            >
              <CheckCircle2 className="h-3 w-3 text-emerald-500" />
              {ext}
            </Badge>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
