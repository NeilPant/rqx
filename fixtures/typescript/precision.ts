/**
 * @author someone
 * Created on 2019-04-02
 */
export function widen(input: any): string {
  const raw = input as string;
  return raw!.trim();
}

// user name
export const userName = "ada";

// imprecise-type: two casts in a row are one decision, so this reports once.
export function forced(input: string): number {
  return input as unknown as number;
}

export function narrowed() {
  return { kind: "ok" } as const;
}
