<script lang="ts">
	import { goto } from '$app/navigation';
	import { errorMessage } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let username = $state('');
	let password = $state('');
	let confirm = $state('');
	let busy = $state(false);
	// Without an administrator yet the page can only create one; otherwise it toggles sign in / sign up.
	let signingUp = $state(false);
	const creating = $derived(data.setupRequired || signingUp);
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
			if (data.setupRequired) await session.createAdmin(username, password);
			else if (signingUp) await session.register(username, password);
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
		<h1>{data.setupRequired ? 'Create the administrator' : signingUp ? 'Create an account' : 'Sign in'}</h1>
		{#if data.setupRequired}
			<p class="hint">First run: this account manages settings, the certificate and the other users.</p>
		{/if}
		{#if data.unreachable}<div class="notice error">Could not reach the server.</div>{/if}
		{#if error}<div class="notice error">{error}</div>{/if}
		<div class="row" style="grid-template-columns: 1fr">
			<div>
				<label for="username">Username</label>
				<input id="username" autocomplete="username" autocapitalize="none" bind:value={username} required />
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
			<button disabled={busy}>{busy ? 'Please wait…' : data.setupRequired ? 'Create and sign in' : 'Sign in'}</button>
			{#if !data.setupRequired}
				<button type="button" class="secondary" onclick={() => { signingUp = !signingUp; error = null; }}>
					{signingUp ? 'I already have an account' : 'Create an account'}
				</button>
			{/if}
		</div>
	</form>
</div>
