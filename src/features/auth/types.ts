export interface AuthenticatedUser {
  id: string;
  fullName: string;
  username: string;
  email: string;
  roles: string[];
}

export interface AnonymousSession {
  authenticated: false;
}

export interface AuthenticatedSession {
  authenticated: true;
  user: AuthenticatedUser;
  absoluteExpiresAt: string;
  idleExpiresAt: string;
}

export type AuthSession = AnonymousSession | AuthenticatedSession;

export interface LoginInput {
  username: string;
  password: string;
}

export interface AuthService {
  getSession(): Promise<AuthSession>;
  login(input: LoginInput): Promise<AuthenticatedSession>;
  logout(): Promise<void>;
  rotateCsrf(): Promise<string>;
}
