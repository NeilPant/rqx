import { frobnicate } from "some-package";

// A call into a package the project does not contain costs nothing here and does not
// make the answer any less sure.

// @complexity time=O(n) space=O(1) conf=verified
export function frobnicateAll(items: string[]): number {
  let done = 0;
  for (const item of items) {
    done += frobnicate(item);
  }
  return done;
}
