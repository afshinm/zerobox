import type { ClientConfig } from './types.js';

export class ZeroboxClient {
  private endpoint: string;
  private token?: string;

  constructor(config: ClientConfig) {
    this.endpoint = config.endpoint.replace(/\/$/, '');
    this.token = config.token;
  }

  private headers(): Record<string, string> {
    const h: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.token) h['Authorization'] = `Bearer ${this.token}`;
    return h;
  }

  async get<T>(path: string): Promise<T> {
    const res = await fetch(`${this.endpoint}${path}`, { headers: this.headers() });
    if (!res.ok) throw new Error(`GET ${path} failed: ${res.status}`);
    return res.json() as Promise<T>;
  }

  async post<T>(path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.endpoint}${path}`, {
      method: 'POST',
      headers: this.headers(),
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) throw new Error(`POST ${path} failed: ${res.status}`);
    return res.json() as Promise<T>;
  }

  async del(path: string): Promise<void> {
    const res = await fetch(`${this.endpoint}${path}`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`DELETE ${path} failed: ${res.status}`);
  }
}
