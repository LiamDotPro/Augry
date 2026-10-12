/** Invite codes are XXXXX-XXXXX from this alphabet (no 0, 1, I, L or O). */
const CODE_CHARS = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE = new RegExp(`^[${CODE_CHARS}]{10}$`);

/**
 * The invite code in whatever was pasted: a full augry.app/join/… link, an
 * augry://join/… link, or the bare code with or without the hyphen, any case.
 * Returns it as XXXXX-XXXXX, or null when there isn't one.
 */
export function parseInviteCode(input: string): string | null {
  let text = input.trim();
  const link = /(?:augry\.app\/join\/|augry:\/\/join\/)([^/?#\s]+)/i.exec(text);
  if (link) text = decodeURIComponent(link[1]!);
  const bare = text.replace(/[\s-]/g, "").toUpperCase();
  if (!CODE.test(bare)) return null;
  return `${bare.slice(0, 5)}-${bare.slice(5)}`;
}

/** The code in an augry://join/<code> deep link, or null for any other link. */
export function inviteFromDeepLink(url: string): string | null {
  if (!/^augry:\/\/join\//i.test(url.trim())) return null;
  return parseInviteCode(url);
}
