<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { customers, errorMessage } from '#lib/api.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const isNew = $derived(page.params.id === 'new');

	// The form starts from the loaded customer; the page is remounted when navigating.
	/* svelte-ignore state_referenced_locally */
	const c = data.customer;
	let idType = $state(c?.id_type ?? '05');
	let idNumber = $state(c?.id_number ?? '');
	let name = $state(c?.business_name ?? '');
	let address = $state(c?.address ?? '');
	let phone = $state(c?.phone ?? '');
	let email = $state(c?.email ?? '');
	let busy = $state(false);
	/* svelte-ignore state_referenced_locally */
	let error = $state<string | null>(data.error);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		const body = {
			id_type: idType,
			id_number: idNumber,
			business_name: name,
			address: address || null,
			phone: phone || null,
			email: email || null
		};
		busy = true;
		try {
			if (isNew) await customers.create(body);
			else await customers.update(Number(page.params.id), body);
			await goto('/customers');
		} catch (e) {
			error = errorMessage(e);
		}
		busy = false;
	}
</script>

<a class="back" href="/customers">← Customers</a>
<div class="page-header"><h1>{isNew ? 'New customer' : 'Edit customer'}</h1></div>

{#if error}<div class="notice error">{error}</div>{/if}

<form class="card" onsubmit={submit}>
	<div class="row">
		<div>
			<label for="t">ID type</label>
			<select id="t" bind:value={idType}>
				<option value="05">Cédula</option><option value="04">RUC</option>
				<option value="06">Passport</option><option value="08">Foreign ID</option>
			</select>
		</div>
		<div><label for="i">ID number</label><input id="i" autocomplete="off" bind:value={idNumber} required /></div>
	</div>
	<div class="row">
		<div style="grid-column: span 2"><label for="n">Name / business name</label><input id="n" bind:value={name} required /></div>
	</div>
	<div class="row">
		<div><label for="a">Address (optional)</label><input id="a" bind:value={address} /></div>
		<div><label for="p">Phone (optional)</label><input id="p" bind:value={phone} /></div>
		<div><label for="e">Email (optional)</label><input id="e" type="email" bind:value={email} /></div>
	</div>
	<div class="actions"><button disabled={busy}>{busy ? 'Saving…' : 'Save'}</button><a class="btn secondary" href="/customers">Cancel</a></div>
</form>
