<script lang="ts">
	import { goto } from '$app/navigation';
	import { errorMessage, inTauri } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let username = $state('');
	let password = $state('');
	let confirm = $state('');
	let busy = $state(false);
	// Without an administrator yet the page can only create one; otherwise it toggles sign in / sign up.
	// svelte-ignore state_referenced_locally
	// On the desktop app the first account is the business owner, so it starts with the RUC form.
	let signingUp = $state(inTauri() && data.setupRequired);
	const creating = $derived(data.setupRequired || signingUp);
	const heading = $derived(
		signingUp ? 'Create an account' : data.setupRequired ? 'Create the administrator' : 'Sign in'
	);
	let error = $state<string | null>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		if (creating && password !== confirm) {
			error = 'The passwords do not match';
			return;
		}
		busy = true;
		try {
			if (signingUp) await session.register(username, password);
			else if (data.setupRequired) await session.createAdmin(username, password);
			else await session.signIn(username, password);
			await goto('/');
		} catch (e) {
			error = errorMessage(e);
		}
		busy = false;
	}
</script>

<div class="login">
	<form class="card" onsubmit={submit}>
		<h1>{heading}</h1>
		{#if data.setupRequired && !signingUp}
			<p class="hint">First run: this account manages the accounts of this server.</p>
		{/if}
		{#if data.unreachable}<div class="notice error">Could not reach the server.</div>{/if}
		{#if error}<div class="notice error">{error}</div>{/if}
		<div class="row" style="grid-template-columns: 1fr">
			<div>
				<label for="username">{signingUp ? 'RUC' : 'Username (or RUC)'}</label>
				<input id="username" autocomplete="username" autocapitalize="none" inputmode={signingUp ? 'numeric' : 'text'} maxlength={signingUp ? 13 : undefined} bind:value={username} required />
				{#if signingUp}<p class="hint">Your business data is fetched from the SRI. Afterwards you only need to upload your signature (.p12).</p>{/if}
			</div>
			<div>
				<label for="password">Password</label>
				<input id="password" type="password" autocomplete={creating ? 'new-password' : 'current-password'} bind:value={password} required />
			</div>
			{#if creating}
				<div>
					<label for="confirm">Repeat the password</label>
					<input id="confirm" type="password" autocomplete="new-password" bind:value={confirm} required />
				</div>
			{/if}
		</div>
		<div class="actions" style="margin-top: 1rem">
			<button disabled={busy}>{busy ? (signingUp ? 'Checking with the SRI…' : 'Please wait…') : signingUp ? 'Create account' : data.setupRequired ? 'Create and sign in' : 'Sign in'}</button>
			<button type="button" class="secondary" onclick={() => { signingUp = !signingUp; error = null; }}>
				{#if data.setupRequired}
					{signingUp ? 'Create a plain administrator instead' : 'Register with my RUC instead'}
				{:else}
					{signingUp ? 'I already have an account' : 'Create an account'}
				{/if}
			</button>
		</div>
	</form>
</div>
