/** Lower case hex SHA-256 of the text's UTF-8 bytes, as the API computes it. */
export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** UTF-8 size, to tell a changed file from an unchanged one without reading it. */
export function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}
