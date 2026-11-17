// A function that other files reach through a barrel.

// @complexity time=O(n) space=O(1) conf=verified
export function find(list: string[], wanted: string): number {
  for (let i = 0; i < list.length; i++) {
    if (list[i] === wanted) {
      return i;
    }
  }
  return -1;
}
