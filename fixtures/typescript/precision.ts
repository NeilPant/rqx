export function widen(input: any): string {
  const raw = input as string;
  return raw!.trim();
}

export const userName = "ada";

export function narrowed() {
  return { kind: "ok" } as const;
}
