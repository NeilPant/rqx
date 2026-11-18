// mixed-language-file: a repository is where the SQL belongs.
export function accountById(id: string): string {
  return `SELECT id, owner FROM accounts WHERE id = '${id}'`;
}
