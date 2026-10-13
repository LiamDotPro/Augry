/**
 * How notes sync treats the server's answers over time, with no IO: which
 * failures are "try again later", how long to wait, and the pace that keeps a
 * big first sync under the API's rate limit (10 requests a second per IP,
 * bursts of 30, then 429 with Retry-After; apps/api/internal/httpapi/ratelimit.go).
 */

/**
 * Answers that say nothing about the note: slow down (429), too slow (408),
 * too early (425). The change stays waiting and the round stops. 5xx and no
 * answer at all are "offline" (CloudError.offline), which also retries.
 */
export function isTransient(status: number): boolean {
  return status === 408 || status === 425 || status === 429;
}

/**
 * Answers that refuse this note as it is: bad request (a path or a size rule,
 * the space's document limit), too large, unprocessable. It stays on this
 * computer until it changes.
 */
export function isRefusal(status: number): boolean {
  return status === 400 || status === 413 || status === 422;
}

const FIRST_RETRY_MS = 2_000;
const LONGEST_RETRY_MS = 60_000;

/**
 * How long to wait after the `attempt`th transient answer in a row (1 for
 * the first): 2 s, doubling to a minute. The server's Retry-After is whole
 * seconds and at least 1, but cloud_request only hands back the status and
 * the body, so the wait starts a little above it and grows while the server
 * keeps saying no.
 */
export function retryDelayMs(attempt: number): number {
  const step = Math.max(0, Math.min(attempt - 1, 10));
  return Math.min(FIRST_RETRY_MS * 2 ** step, LONGEST_RETRY_MS);
}

/**
 * A token bucket like the server's, a little slower, so sync leaves room for
 * the rest of the app: `take` answers how long to wait before the next call.
 */
export class Pace {
  private tokens: number;
  private at: number;

  constructor(
    private readonly perSecond: number,
    private readonly burst: number,
    now: number,
  ) {
    this.tokens = burst;
    this.at = now;
  }

  /** Takes one token, now or after the wait it returns (ms). */
  take(now: number): number {
    this.tokens = Math.min(this.burst, this.tokens + ((now - this.at) / 1000) * this.perSecond);
    this.at = now;
    this.tokens -= 1;
    return this.tokens >= 0 ? 0 : Math.ceil((-this.tokens / this.perSecond) * 1000);
  }
}
