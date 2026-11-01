import { getJson } from "./http";
import type { User } from "./types";

/** Profile lookup used by the old settings page. */
export function fetchUser(userId: number): Promise<User> {
  return getJson<User>(`/v1/users/${userId}`);
}
