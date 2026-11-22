/**
 * Backend API client. Works in the browser and in the desktop app:
 *  - in Tauri it calls the backend inside the same process (`api` command, no network, no token);
 *  - on the web it goes through `/api/*`, a SvelteKit server proxy that adds the token.
 */
export type VatCode = '15' | '5' | '0' | 'not_subject' | 'exempt';

export type InvoiceRecord = {
	access_key: string;
	number: string;
	sequence: number;
	issue_date: string | null;
	document_type: string;
	id_number: string | null;
	business_name: string | null;
	/** In cents. */
	total_amount: number | null;
	status: string;
	authorization_number: string | null;
	authorization_date: string | null;
	messages: string | null;
	signed_xml?: string;
};

export type InvoiceLine = {
	main_code: string;
	description: string;
	quantity: number;
	unit_price_cents: number;
	discount_cents: number;
	vat: VatCode;
};

export type NewInvoice = {
	buyer?: { id_type: string; business_name: string; id_number: string; address?: string };
	lines: InvoiceLine[];
	payment_method: string;
	additional_info: [string, string][];
};

export class ApiError extends Error {
	constructor(
		public status: number,
		message: string
	) {
		super(message);
	}
}

declare global {
	interface Window {
		__TAURI_INTERNALS__?: unknown;
	}
}

export const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type RawResponse = { status: number; body: string };

/** Called when the server says the session is no longer valid (set by the session module). */
let onUnauthorized: () => void = () => {};
export const setUnauthorizedHandler = (handler: () => void) => (onUnauthorized = handler);

const SESSION_KEY = 'session';

/** The signed-in session, kept in localStorage so it survives restarts. */
export type StoredSession = { token: string; expires_at: number; user: User };

export function readSession(): StoredSession | null {
	try {
		const raw = localStorage.getItem(SESSION_KEY);
		if (!raw) return null;
		const s = JSON.parse(raw) as StoredSession;
		return s.expires_at * 1000 > Date.now() ? s : null;
	} catch {
		return null;
	}
}

export function writeSession(s: StoredSession | null) {
	try {
		if (s) localStorage.setItem(SESSION_KEY, JSON.stringify(s));
		else localStorage.removeItem(SESSION_KEY);
	} catch {
		/* storage unavailable: the session just won't persist */
	}
}

async function transport(method: string, path: string, body?: string): Promise<RawResponse> {
	const token = readSession()?.token ?? null;
	if (inTauri()) {
		const { invoke } = await import('@tauri-apps/api/core');
		try {
			return await invoke<RawResponse>('api', { method, path, body: body ?? null, token });
		} catch (e) {
			throw new ApiError(500, String(e));
		}
	}
	const headers: Record<string, string> = {};
	if (body) headers['content-type'] = 'application/json';
	if (token) headers.authorization = `Bearer ${token}`;
	let res: Response;
	try {
		res = await fetch(`/api${path}`, { method, headers, body });
	} catch {
		throw new ApiError(503, 'Could not reach the server');
	}
	return { status: res.status, body: await res.text() };
}

function parse<T>(r: RawResponse, isLoginPath = false): T {
	let data: unknown = null;
	try {
		data = r.body ? JSON.parse(r.body) : null;
	} catch {
		/* non-JSON body (e.g. CSV) is handled by the caller */
	}
	if (r.status === 401 && !isLoginPath) onUnauthorized();
	if (r.status < 200 || r.status >= 300) {
		const message = (data as { error?: string } | null)?.error;
		throw new ApiError(r.status, message ?? `Error ${r.status}`);
	}
	return data as T;
}

async function call<T>(path: string, init: { method?: string; body?: unknown } = {}): Promise<T> {
	const body = init.body === undefined ? undefined : JSON.stringify(init.body);
	// A wrong password also answers 401: that must not be taken as an expired session.
	const isLoginPath = path === '/auth/login' || path === '/auth/setup' || path === '/auth/register';
	return parse<T>(await transport(init.method ?? 'GET', path, body), isLoginPath);
}

export function queryString(params: Record<string, string | number | undefined>): string {
	const p = new URLSearchParams();
	for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== '') p.set(k, String(v));
	const s = p.toString();
	return s ? `?${s}` : '';
}

// ---------- accounts ----------

export type Role = 'admin' | 'operator';
export type User = { id: number; username: string; role: Role; active: boolean };
export type Login = { token: string; expires_at: number; user: User };

export const authStatus = () => call<{ setup_required: boolean }>('/auth/status');

export const setupAdmin = (username: string, password: string) =>
	call<Login>('/auth/setup', { method: 'POST', body: { username, password } });

export const register = (username: string, password: string) =>
	call<Login>('/auth/register', { method: 'POST', body: { username, password } });

export const login = (username: string, password: string) =>
	call<Login>('/auth/login', { method: 'POST', body: { username, password } });

export const logoutRequest = () => call<null>('/auth/logout', { method: 'POST' });

export const changePassword = (current_password: string, new_password: string) =>
	call<null>('/auth/password', { method: 'PUT', body: { current_password, new_password } });

export const users = {
	list: () => call<User[]>('/users'),
	create: (u: { username: string; password: string; role: Role }) =>
		call<User>('/users', { method: 'POST', body: u }),
	update: (id: number, changes: { role?: Role; active?: boolean; password?: string }) =>
		call<User>(`/users/${id}`, { method: 'PUT', body: changes })
};

// ---------- invoices ----------

export type InvoiceSearch = {
	date_from?: string;
	date_to?: string;
	id_number?: string;
	status?: string;
	access_key?: string;
	limit?: number;
	page?: number;
};

export const searchInvoices = (s: InvoiceSearch) =>
	call<{ total: number; items: InvoiceRecord[] }>(`/invoices${queryString(s)}`);

/** CSV report with the same filters (the backend caps it at 10,000 rows). */
export async function invoiceReportCsv(s: InvoiceSearch): Promise<string> {
	const r = await transport('GET', `/invoices/report.csv${queryString(s)}`);
	if (r.status === 401) onUnauthorized();
	if (r.status !== 200) throw new ApiError(r.status, `Error ${r.status}`);
	return r.body;
}

export const getInvoice = (accessKey: string) =>
	call<InvoiceRecord>(`/invoices/${encodeURIComponent(accessKey)}`);

export const issueInvoice = (invoice: NewInvoice) =>
	call<InvoiceRecord>('/invoices', { method: 'POST', body: invoice });

export const retryPending = () =>
	call<{ access_key: string; status?: string; error?: string }[]>('/invoices/retry', {
		method: 'POST'
	});

// ---------- settings ----------

export type Issuer = {
	ruc: string;
	business_name: string;
	trade_name: string | null;
	head_office_address: string;
	branch_address: string | null;
	accounting_required: boolean;
	establishment: string;
	emission_point: string;
};

export type Environment = 'test' | 'production';

export type Config = {
	issuer: Issuer | null;
	environment: Environment;
	certificate: { loaded: boolean; subject?: string; expires?: string; valid?: boolean; error?: string };
	ready: boolean;
};

export const getConfig = () => call<Config>('/config');

export const saveIssuer = (issuer: Issuer & { environment: Environment }) =>
	call<null>('/config/issuer', { method: 'PUT', body: issuer });

export const saveCertificate = (p12Base64: string, password: string) =>
	call<{ subject: string; expires: string }>('/config/certificate', {
		method: 'PUT',
		body: { p12_base64: p12Base64, password }
	});

// ---------- catalogs ----------

export type Product = {
	id: number;
	main_code: string;
	aux_code: string | null;
	name: string;
	price_cents: number;
	vat: VatCode;
};

export type Customer = {
	id: number;
	id_type: string;
	id_number: string;
	business_name: string;
	address: string | null;
	phone: string | null;
	email: string | null;
};

type CatalogQuery = { q?: string; limit?: number; page?: number };

function catalog<T extends { id: number }>(base: string) {
	return {
		list: (p: CatalogQuery = {}) =>
			call<{ total: number; items: T[] }>(`${base}${queryString(p)}`),
		get: (id: number) => call<T>(`${base}/${id}`),
		create: (item: Omit<T, 'id'>) => call<T>(base, { method: 'POST', body: item }),
		update: (id: number, item: Omit<T, 'id'>) =>
			call<T>(`${base}/${id}`, { method: 'PUT', body: item }),
		remove: (id: number) => call<null>(`${base}/${id}`, { method: 'DELETE' })
	};
}

export const products = catalog<Product>('/products');
export const customers = catalog<Customer>('/customers');

/** Turns any thrown value into a message that can be shown to the user. */
export const errorMessage = (e: unknown): string =>
	e instanceof ApiError ? e.message : 'Unexpected error';
