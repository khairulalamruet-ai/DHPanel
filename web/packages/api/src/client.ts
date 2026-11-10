let csrfToken: string | null = null;

export function setCsrfToken(token: string | null) {
  csrfToken = token;
}

export function getCsrfToken(): string | null {
  return csrfToken;
}

export interface ApiFetchOptions extends RequestInit {
  params?: Record<string, string | number | boolean | undefined>;
}

export async function apiFetch<T>(path: string, options: ApiFetchOptions = {}): Promise<T> {
  const { params, headers = {}, ...rest } = options;

  let url = path;
  if (params) {
    const searchParams = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) {
        searchParams.append(key, String(value));
      }
    }
    const queryString = searchParams.toString();
    if (queryString) {
      url += (url.includes("?") ? "&" : "?") + queryString;
    }
  }

  const reqHeaders: Record<string, string> = {
    "Accept": "application/json",
    ...(headers as Record<string, string>),
  };

  const isMutating = ["POST", "PUT", "PATCH", "DELETE"].includes((rest.method || "GET").toUpperCase());
  if (isMutating && csrfToken) {
    reqHeaders["X-CSRF-Token"] = csrfToken;
  }

  if (rest.body && typeof rest.body === "string" && !reqHeaders["Content-Type"]) {
    reqHeaders["Content-Type"] = "application/json";
  }

  const response = await fetch(url, {
    ...rest,
    headers: reqHeaders,
    credentials: "include", // Session HttpOnly cookie
  });

  if (!response.ok) {
    let errorBody: any;
    try {
      errorBody = await response.json();
    } catch {
      errorBody = { message: response.statusText };
    }
    const err = new Error(errorBody.message || errorBody.title || "API request failed");
    (err as any).status = response.status;
    (err as any).details = errorBody;
    throw err;
  }

  if (response.status === 204) {
    return {} as T;
  }

  return response.json();
}
