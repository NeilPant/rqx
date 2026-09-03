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

export function narrowed() {
  return { kind: "ok" } as const;
}
