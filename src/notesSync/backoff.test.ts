import { describe, expect, it } from "vitest";
import { isRefusal, isTransient, Pace, retryDelayMs } from "./backoff";

describe("isTransient and isRefusal", () => {
  it("waits on 429 and 408, and refuses only what is about the note", () => {
    expect(isTransient(429)).toBe(true);
    expect(isTransient(408)).toBe(true);
    expect(isRefusal(429)).toBe(false);
    for (const status of [400, 413, 422]) {
      expect(isRefusal(status)).toBe(true);
      expect(isTransient(status)).toBe(false);
    }
    for (const status of [401, 403, 404, 409, 410]) {
      expect(isRefusal(status)).toBe(false);
      expect(isTransient(status)).toBe(false);
    }
  });
});

describe("retryDelayMs", () => {
  it("starts at 2 s and doubles to a minute", () => {
    expect([1, 2, 3, 4, 5, 6, 7, 50].map(retryDelayMs)).toEqual([
      2_000, 4_000, 8_000, 16_000, 32_000, 60_000, 60_000, 60_000,
    ]);
    expect(retryDelayMs(0)).toBe(2_000);
  });
});

describe("Pace", () => {
  it("lets a burst through, then spaces calls at its rate", () => {
    const pace = new Pace(10, 3, 0);
    expect([pace.take(0), pace.take(0), pace.take(0)]).toEqual([0, 0, 0]);
    expect(pace.take(0)).toBe(100);
    // The caller waited 100 ms: the next one waits another 100.
    expect(pace.take(100)).toBe(100);
  });

  it("refills while idle, up to the burst", () => {
    const pace = new Pace(10, 3, 0);
    for (let i = 0; i < 3; i++) pace.take(0);
    expect(pace.take(60_000)).toBe(0);
    expect(pace.take(60_000)).toBe(0);
    expect(pace.take(60_000)).toBe(0);
    expect(pace.take(60_000)).toBe(100);
  });
});
