<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { errorMessage, users, type Role, type User } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let username = $state('');
	let password = $state('');
	let role = $state<Role>('operator');
	let busy = $state(false);
	let notice = $state<{ kind: 'error' | 'info'; text: string } | null>(null);

	async function run(action: () => Promise<unknown>, success: string) {
		notice = null;
		try {
			await action();
			notice = { kind: 'info', text: success };
			await invalidateAll();
		} catch (e) {
			notice = { kind: 'error', text: errorMessage(e) };
		}
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		await run(() => users.create({ username, password, role }), `User "${username}" created.`);
		if (notice?.kind === 'info') username = password = '';
		busy = false;
	}

	const toggleActive = (u: User) =>
		run(() => users.update(u.id, { active: !u.active }), `${u.username} ${u.active ? 'deactivated' : 'activated'}.`);

	const toggleRole = (u: User) => {
		const next: Role = u.role === 'admin' ? 'operator' : 'admin';
		return run(() => users.update(u.id, { role: next }), `${u.username} is now ${next}.`);
	};

	function resetPassword(u: User) {
		const value = prompt(`New password for "${u.username}" (at least 8 characters). Their open sessions will be closed.`);
		if (value) void run(() => users.update(u.id, { password: value }), `Password of ${u.username} changed.`);
	}
</script>

<div class="page-header">
	<div>
		<h1>Users</h1>
		<p>Every account is its own business, with its own RUC, signature, catalogs and invoices. Administrators also manage the accounts.</p>
	</div>
</div>

{#if data.error}<div class="notice error">{data.error}</div>{/if}
{#if notice}<div class="notice {notice.kind}">{notice.text}</div>{/if}

<form class="card" onsubmit={create}>
	<h2>New user</h2>
	<div class="row">
		<div><label for="username">Username</label><input id="username" autocomplete="off" autocapitalize="none" bind:value={username} required /></div>
		<div><label for="password">Password</label><input id="password" type="password" autocomplete="new-password" minlength="8" bind:value={password} required /></div>
		<div>
			<label for="role">Role</label>
			<select id="role" bind:value={role}>
				<option value="operator">Member</option>
				<option value="admin">Administrator</option>
			</select>
		</div>
	</div>
	<button disabled={busy}>{busy ? 'Creating…' : 'Create user'}</button>
</form>

<div class="card flush table-wrap">
	<table>
		<thead><tr><th>Username</th><th>Role</th><th>Status</th><th></th></tr></thead>
		<tbody>
			{#each data.items as u (u.id)}
				<tr>
					<td>{u.username}{u.id === session.user?.id ? ' (you)' : ''}</td>
					<td>{u.role === 'admin' ? 'Administrator' : 'Member'}</td>
					<td><span class="badge {u.active ? 'AUTORIZADO' : 'RECHAZADO'}">{u.active ? 'ACTIVE' : 'INACTIVE'}</span></td>
					<td class="right">
						<span class="actions" style="justify-content:flex-end">
							<button class="secondary small" onclick={() => toggleRole(u)}>Make {u.role === 'admin' ? 'member' : 'admin'}</button>
							<button class="secondary small" onclick={() => resetPassword(u)}>Reset password</button>
							<button class="secondary small {u.active ? 'danger' : ''}" onclick={() => toggleActive(u)}>{u.active ? 'Deactivate' : 'Activate'}</button>
						</span>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>
