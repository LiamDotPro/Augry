// Client for the Augry cloud API. The types in schema.d.ts are generated from
// libs/api-contract/openapi.yaml (`npm run api:generate`); nothing here is
// written per route.
//
//   const features = $api.useQuery("get", "/v1/features");
//   const install = $api.useMutation("post", "/v1/templates/{slug}/install");
//   const { data, error } = await cloud.GET("/v1/templates");
//
// The Rust side has the same client in src-tauri/src/cloud.
import createClient, { type Middleware } from "openapi-fetch";
import createQueryClient from "openapi-react-query";
import { QueryClient } from "@tanstack/react-query";
import type { components, paths } from "./schema";

export type Schemas = components["schemas"];

/**
 * The production API. VITE_AUGRY_API_URL overrides it, e.g. http://localhost:8080.
 * Fly's own hostname until the api.augry.app certificate is verified.
 */
export const DEFAULT_BASE_URL = "https://augry-api.fly.dev";

export function createCloudClient(baseUrl: string = import.meta.env.VITE_AUGRY_API_URL ?? DEFAULT_BASE_URL) {
  return createClient<paths>({ baseUrl, headers: { Accept: "application/json" } });
}

/** Signs every request of `client` in with the token from `getToken`, while it returns one. */
export function withSessionToken(client: ReturnType<typeof createCloudClient>, getToken: () => string | null) {
  const auth: Middleware = {
    onRequest({ request }) {
      const token = getToken();
      if (token) request.headers.set("Authorization", `Bearer ${token}`);
      return request;
    },
  };
  client.use(auth);
  return () => client.eject(auth);
}

export const cloud = createCloudClient();
export const $api = createQueryClient(cloud);
export const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: 1, staleTime: 30_000 } },
});
