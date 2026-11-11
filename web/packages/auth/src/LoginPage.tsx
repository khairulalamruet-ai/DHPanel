import React, { useState } from "react";
import {
  Button,
  Input,
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "@dhpanel/ui";
import { useTranslation, LanguageSwitcher } from "@dhpanel/i18n";
import { api, setCsrfToken } from "@dhpanel/api";
import { useSession } from "./context";
import { ShieldCheck, Lock, User, KeyRound, AlertCircle, ArrowLeft } from "lucide-react";

export interface LoginPageProps {
  title?: string;
  onSuccess?: () => void;
}

export function LoginPage({ title, onSuccess }: LoginPageProps) {
  const { t } = useTranslation();
  const { refetch } = useSession();

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [totpCode, setTotpCode] = useState("");
  const [step, setStep] = useState<"credentials" | "totp">("credentials");

  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMessage(null);

    if (step === "credentials") {
      if (!username.trim() || !password) {
        setErrorMessage(t("auth.invalidCredentials"));
        return;
      }
    } else if (step === "totp") {
      if (!totpCode.trim() || totpCode.length < 6) {
        setErrorMessage(t("auth.invalidTotp"));
        return;
      }
    }

    try {
      setIsLoading(true);
      const res = await api.POST("/api/v1/public/login", {
        body: {
          username: username.trim(),
          password,
          totp_code: step === "totp" ? totpCode.trim() : undefined,
        },
      });

      if (res.response.status === 202 || res.data?.requires_2fa) {
        setStep("totp");
        setErrorMessage(null);
        return;
      }

      if (res.error) {
        const errorData = res.error as any;
        if (errorData?.error === "invalid_2fa") {
          setErrorMessage(t("auth.invalidTotp"));
        } else {
          setErrorMessage(t("auth.invalidCredentials"));
        }
        return;
      }

      if (res.data?.csrf_token) {
        setCsrfToken(res.data.csrf_token);
      }

      await refetch();
      if (onSuccess) {
        onSuccess();
      }
    } catch {
      setErrorMessage(t("common.error"));
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex flex-col justify-center items-center bg-muted/30 px-4 py-8 relative">
      <div className="absolute top-4 right-4 flex items-center gap-2">
        <LanguageSwitcher />
      </div>

      <div className="w-full max-w-md space-y-6">
        <div className="flex flex-col items-center space-y-2 text-center">
          <div className="h-12 w-12 rounded-xl bg-primary/10 flex items-center justify-center text-primary mb-2 shadow-sm">
            <ShieldCheck className="h-7 w-7" />
          </div>
          <h1 className="text-2xl font-bold tracking-tight">
            {title || t("common.appName")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {step === "credentials" ? t("auth.signInDesc") : t("auth.totpDesc")}
          </p>
        </div>

        <Card className="shadow-lg border-muted">
          <CardHeader className="space-y-1">
            <CardTitle className="text-xl">
              {step === "credentials" ? t("auth.signInTitle") : t("auth.totpCode")}
            </CardTitle>
            <CardDescription>
              {step === "credentials"
                ? t("auth.signInDesc")
                : t("auth.totpDesc")}
            </CardDescription>
          </CardHeader>

          <form onSubmit={handleSubmit} noValidate>
            <CardContent className="space-y-4">
              {errorMessage && (
                <div
                  role="alert"
                  className="rounded-lg bg-destructive/15 border border-destructive/30 p-3 text-sm text-destructive flex items-center gap-2"
                >
                  <AlertCircle className="h-4 w-4 shrink-0" />
                  <span>{errorMessage}</span>
                </div>
              )}

              {step === "credentials" ? (
                <>
                  <div className="space-y-1.5">
                    <label
                      htmlFor="username"
                      className="text-xs font-semibold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5"
                    >
                      <User className="h-3.5 w-3.5" />
                      {t("auth.username")}
                    </label>
                    <Input
                      id="username"
                      name="username"
                      type="text"
                      placeholder={t("auth.usernamePlaceholder")}
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                      disabled={isLoading}
                      autoComplete="username"
                      autoFocus
                      required
                    />
                  </div>

                  <div className="space-y-1.5">
                    <label
                      htmlFor="password"
                      className="text-xs font-semibold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5"
                    >
                      <Lock className="h-3.5 w-3.5" />
                      {t("auth.password")}
                    </label>
                    <Input
                      id="password"
                      name="password"
                      type="password"
                      placeholder={t("auth.passwordPlaceholder")}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      disabled={isLoading}
                      autoComplete="current-password"
                      required
                    />
                  </div>
                </>
              ) : (
                <div className="space-y-1.5">
                  <label
                    htmlFor="totpCode"
                    className="text-xs font-semibold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5"
                  >
                    <KeyRound className="h-3.5 w-3.5" />
                    {t("auth.totpCode")}
                  </label>
                  <Input
                    id="totpCode"
                    name="totpCode"
                    type="text"
                    inputMode="numeric"
                    maxLength={6}
                    placeholder={t("auth.totpPlaceholder")}
                    value={totpCode}
                    onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ""))}
                    disabled={isLoading}
                    autoComplete="one-time-code"
                    autoFocus
                    required
                  />
                </div>
              )}
            </CardContent>

            <CardFooter className="flex flex-col gap-2 pt-2">
              <Button type="submit" className="w-full" disabled={isLoading}>
                {isLoading ? t("auth.verifying") : t("auth.submit")}
              </Button>

              {step === "totp" && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="w-full text-xs text-muted-foreground gap-1"
                  onClick={() => {
                    setStep("credentials");
                    setErrorMessage(null);
                  }}
                  disabled={isLoading}
                >
                  <ArrowLeft className="h-3.5 w-3.5" />
                  {t("auth.backToLogin")}
                </Button>
              )}
            </CardFooter>
          </form>
        </Card>
      </div>
    </div>
  );
}
