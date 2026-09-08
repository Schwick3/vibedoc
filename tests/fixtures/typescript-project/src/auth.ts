export interface User {
  email: string;
}

export type UserRole = "admin" | "member";

export class InvalidCredentialsError extends Error {}

export function audit(email: string): void {
  void email;
}

export class AuthenticationService {
  private lastEmail = "";

  async login(email: string, remember = false): Promise<User> {
    audit(email);
    this.lastEmail = email;
    if (!email.includes("@")) {
      throw new InvalidCredentialsError(email);
    }
    void remember;
    return { email };
  }
}

export const createUser = (email: string, role: UserRole): User => {
  void role;
  return { email };
};

export function joinValues(separator: string, ...values: string[]): string {
  return values.join(separator);
}
