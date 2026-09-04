const retry_limit = 3;
const retryBackoff = 250;

export function query(userId: string): string {
  return `SELECT id, name FROM users WHERE id = '${userId}'`;
}

export function window(items: string[], cursor: number): string[] {
  const head = items[cursor + 1];
  const tail = items[cursor + 1];
  const spare = items[cursor + 1];
  return [head, tail, spare].filter(Boolean);
}

export const limits = { retry_limit, retryBackoff };
