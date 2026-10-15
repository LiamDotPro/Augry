import { describe, expect, it } from "vitest";
import { createCloudClient } from "./api";

// Talks to a running Augry API. Skipped unless VITE_AUGRY_API_URL is set, so
// `npm test` stays offline. Run with `npm run test:cloud`.
const baseUrl: string | undefined = import.meta.env.VITE_AUGRY_API_URL;

describe.skipIf(!baseUrl)("cloud API", () => {
  const api = createCloudClient(baseUrl);

  it("is up and has its database", async () => {
    expect((await api.GET("/healthz")).data).toEqual({ status: "ok" });
    expect((await api.GET("/readyz")).data).toEqual({ status: "ok" });
  });

  it("lists the features available without signing in", async () => {
    const { data } = await api.GET("/v1/features");
    expect(data?.apiVersion).toBe(1);
    expect(data?.features).toContain("templates");
  });

  it("lists the template catalogue", async () => {
    const { data } = await api.GET("/v1/templates");
    expect(Array.isArray(data?.templates)).toBe(true);
    console.log(`templates: ${data!.templates.map((t) => `${t.slug} v${t.version}`).join(", ") || "(none)"}`);
  });

  it("answers signed in routes with a typed 401 error", async () => {
    const { data, error, response } = await api.GET("/v1/me");
    expect(data).toBeUndefined();
    expect(response.status).toBe(401);
    expect(error?.error.code).toBe("unauthorized");
    expect(response.headers.get("X-Request-Id")).toBeTruthy();
  });
});
