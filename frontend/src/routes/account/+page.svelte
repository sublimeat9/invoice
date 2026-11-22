<script lang="ts">
	import { changePassword, errorMessage } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';

	let current = $state('');
	let next = $state('');
	let confirm = $state('');
	let busy = $state(false);
	let result = $state<{ kind: 'error' | 'info'; text: string } | null>(null);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		result = null;
		if (next !== confirm) {
			result = { kind: 'error', text: 'The new passwords do not match' };
			return;
		}
		busy = true;
		try {
			await changePassword(current, next);
			result = { kind: 'info', text: 'Password changed. Your other sessions were closed.' };
			current = next = confirm = '';
		} catch (e) {
			result = { kind: 'error', text: errorMessage(e) };
		}
		busy = false;
	}
</script>

<div class="page-header">
	<div>
		<h1>My account</h1>
		<p>Signed in as <strong>{session.user?.username}</strong> ({session.user?.role === 'admin' ? 'administrator' : 'member'}).</p>
	</div>
</div>

<form class="card" onsubmit={submit}>
	<h2>Change password</h2>
	{#if result}<div class="notice {result.kind}">{result.text}</div>{/if}
	<div class="row">
		<div><label for="current">Current password</label><input id="current" type="password" autocomplete="current-password" bind:value={current} required /></div>
		<div><label for="next">New password</label><input id="next" type="password" autocomplete="new-password" minlength="8" bind:value={next} required /></div>
		<div><label for="confirm">Repeat the new password</label><input id="confirm" type="password" autocomplete="new-password" bind:value={confirm} required /></div>
	</div>
	<button disabled={busy}>{busy ? 'Saving…' : 'Change password'}</button>
	<p class="hint">At least 8 characters.</p>
</form>
