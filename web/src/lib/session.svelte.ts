import { goto } from '$app/navigation';
import {
	login as apiLogin,
	register as apiRegister,
	logoutRequest,
	readSession,
	setupAdmin,
	setUnauthorizedHandler,
	writeSession,
	type Login,
	type User
} from './api.ts';

/** Reactive view of the signed-in user. The token itself lives in localStorage (see api.ts). */
class Session {
	user = $state<User | null>(readSession()?.user ?? null);

	get isAdmin() {
		return this.user?.role === 'admin';
	}

	private start(login: Login) {
		writeSession(login);
		this.user = login.user;
	}

	async signIn(username: string, password: string) {
		this.start(await apiLogin(username, password));
	}

	async register(ruc: string, password: string) {
		this.start(await apiRegister(ruc, password));
	}

	async createAdmin(username: string, password: string) {
		this.start(await setupAdmin(username, password));
	}

	/** Ends the session on the server (best effort) and here. */
	async signOut() {
		try {
			await logoutRequest();
		} catch {
			/* already expired or unreachable: nothing to undo on the server */
		}
		this.clear();
		await goto('/login');
	}

	clear() {
		writeSession(null);
		this.user = null;
	}
}

export const session = new Session();

// An expired or revoked session sends the user to the login page.
setUnauthorizedHandler(() => {
	session.clear();
	void goto('/login');
});
