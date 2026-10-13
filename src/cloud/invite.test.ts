import { describe, expect, it } from "vitest";
import { inviteFromDeepLink, parseInviteCode } from "./invite";

describe("parseInviteCode", () => {
  it("takes the bare code with or without the hyphen, any case", () => {
    expect(parseInviteCode("K7M2P-9XQ4R")).toBe("K7M2P-9XQ4R");
    expect(parseInviteCode("k7m2p9xq4r")).toBe("K7M2P-9XQ4R");
    expect(parseInviteCode("  k7m2p-9xq4r ")).toBe("K7M2P-9XQ4R");
  });

  it("takes the code out of a link", () => {
    expect(parseInviteCode("https://augry.dev/join/K7M2P-9XQ4R")).toBe("K7M2P-9XQ4R");
    expect(parseInviteCode("https://augry.app/join/K7M2P-9XQ4R")).toBe("K7M2P-9XQ4R");
    expect(parseInviteCode("augry.app/join/k7m2p9xq4r?ref=x")).toBe("K7M2P-9XQ4R");
    expect(parseInviteCode("augry://join/K7M2P-9XQ4R")).toBe("K7M2P-9XQ4R");
  });

  it("refuses anything that isn't a code", () => {
    expect(parseInviteCode("")).toBeNull();
    expect(parseInviteCode("K7M2P")).toBeNull();
    // 0, 1, I, L and O are never in a code.
    expect(parseInviteCode("K0M2P-9XQ4R")).toBeNull();
    expect(parseInviteCode("https://augry.app/join/")).toBeNull();
  });
});

describe("inviteFromDeepLink", () => {
  it("only reads augry://join links", () => {
    expect(inviteFromDeepLink("augry://join/K7M2P-9XQ4R")).toBe("K7M2P-9XQ4R");
    expect(inviteFromDeepLink("augry://other/K7M2P-9XQ4R")).toBeNull();
    expect(inviteFromDeepLink("https://augry.app/join/K7M2P-9XQ4R")).toBeNull();
  });
});
