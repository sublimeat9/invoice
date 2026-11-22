<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { errorMessage, saveCertificate, saveIssuer, type Environment } from '#lib/api.ts';
	import { toBase64 } from '#lib/base64.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let config = $derived(data.config);

	type Result = { error?: string; ok?: string } | null;
	let issuerResult = $state<Result>(null);
	let certificateResult = $state<Result>(null);
	let savingIssuer = $state(false);
	let savingCertificate = $state(false);

	// The form starts from the saved issuer; the user's edits live in local state.
	/* svelte-ignore state_referenced_locally */
	const i = data.config?.issuer;
	let ruc = $state(i?.ruc ?? '');
	let businessName = $state(i?.business_name ?? '');
	let tradeName = $state(i?.trade_name ?? '');
	let headOffice = $state(i?.head_office_address ?? '');
	let branch = $state(i?.branch_address ?? '');
	let establishment = $state(i?.establishment ?? '001');
	let emissionPoint = $state(i?.emission_point ?? '001');
	let accountingRequired = $state(i?.accounting_required ?? false);
	/* svelte-ignore state_referenced_locally */
	let environment = $state<Environment>(data.config?.environment ?? 'test');
	let confirmProduction = $state(false);

	let p12File = $state<File | null>(null);
	let password = $state('');

	async function submitIssuer(event: SubmitEvent) {
		event.preventDefault();
		issuerResult = null;
		if (environment === 'production' && !confirmProduction) {
			issuerResult = { error: 'Confirm that you understand production invoices are real.' };
			return;
		}
		savingIssuer = true;
		try {
			await saveIssuer({
				ruc: ruc.trim(),
				business_name: businessName,
				trade_name: tradeName || null,
				head_office_address: headOffice,
				branch_address: branch || null,
				accounting_required: accountingRequired,
				establishment,
				emission_point: emissionPoint,
				environment
			});
			issuerResult = { ok: 'Issuer saved.' };
			await invalidateAll();
		} catch (e) {
			issuerResult = { error: errorMessage(e) };
		}
		savingIssuer = false;
	}

	async function submitCertificate(event: SubmitEvent) {
		event.preventDefault();
		certificateResult = null;
		if (!p12File) return;
		savingCertificate = true;
		try {
			const bytes = new Uint8Array(await p12File.arrayBuffer());
			const info = await saveCertificate(toBase64(bytes), password);
			certificateResult = { ok: `Certificate saved: ${info.subject}` };
			password = '';
			await invalidateAll();
		} catch (e) {
			certificateResult = { error: errorMessage(e) };
		}
		savingCertificate = false;
	}
</script>

<div class="page-header">
	<div>
		<h1>Settings</h1>
		<p>Issuer details, environment and electronic signature certificate.</p>
	</div>
</div>

{#if data.error}<div class="notice error">{data.error}</div>{/if}

{#if config}
	<form class="card" onsubmit={submitIssuer}>
		<h2>Issuer</h2>
		{#if issuerResult?.error}<div class="notice error">{issuerResult.error}</div>{/if}
		{#if issuerResult?.ok}<div class="notice info">{issuerResult.ok}</div>{/if}
		<div class="row">
			<div><label for="ruc">RUC</label><input id="ruc" inputmode="numeric" maxlength="13" bind:value={ruc} required /></div>
			<div style="grid-column: span 2">
				<label for="bn">Business name</label>
				<input id="bn" bind:value={businessName} required />
			</div>
		</div>
		<div class="row">
			<div>
				<label for="tn">Trade name (optional)</label>
				<input id="tn" bind:value={tradeName} />
			</div>
		</div>
		<div class="row">
			<div>
				<label for="ho">Head office address</label>
				<input id="ho" bind:value={headOffice} required />
			</div>
			<div>
				<label for="ba">Branch address (optional)</label>
				<input id="ba" bind:value={branch} />
			</div>
		</div>
		<div class="row">
			<div><label for="es">Establishment</label><input id="es" inputmode="numeric" maxlength="3" bind:value={establishment} required /></div>
			<div><label for="ep">Emission point</label><input id="ep" inputmode="numeric" maxlength="3" bind:value={emissionPoint} required /></div>
			<div style="align-self:end">
				<label class="checkbox-label">
					<input type="checkbox" bind:checked={accountingRequired} />
					Required to keep accounting
				</label>
			</div>
		</div>
		<div class="row">
			<div>
				<label for="env">Environment</label>
				<select id="env" bind:value={environment}>
					<option value="test">Test (no tax validity)</option>
					<option value="production">Production (real invoices)</option>
				</select>
			</div>
		</div>
		{#if environment === 'production'}
			<div class="notice error">
				<label class="checkbox-label">
					<input type="checkbox" bind:checked={confirmProduction} />
					I understand that in production invoices are real and are sent to the SRI.
				</label>
			</div>
		{/if}
		<button disabled={savingIssuer}>{savingIssuer ? 'Saving…' : 'Save issuer'}</button>
		<p class="hint">The sequence number is kept separately for each environment, establishment and emission point.</p>
	</form>

	<form class="card" onsubmit={submitCertificate}>
		<h2>Electronic signature certificate (.p12)</h2>
		{#if certificateResult?.error}<div class="notice error">{certificateResult.error}</div>{/if}
		{#if certificateResult?.ok}<div class="notice info">{certificateResult.ok}</div>{/if}
		{#if config.certificate.loaded && !config.certificate.error}
			<p>
				Current certificate: <strong>{config.certificate.subject}</strong><br />
				Expires {config.certificate.expires}
				<span class="badge {config.certificate.valid ? 'AUTORIZADO' : 'RECHAZADO'}">{config.certificate.valid ? 'VALID' : 'EXPIRED'}</span>
			</p>
		{:else if config.certificate.error}
			<div class="notice error">The stored certificate cannot be used: {config.certificate.error}</div>
		{:else}
			<p>No certificate has been uploaded yet.</p>
		{/if}
		<div class="row">
			<div>
				<label for="p12">.p12 file</label>
				<input id="p12" type="file" accept=".p12,.pfx" required onchange={(e) => (p12File = e.currentTarget.files?.[0] ?? null)} />
			</div>
			<div><label for="pw">Password</label><input id="pw" type="password" autocomplete="off" bind:value={password} required /></div>
		</div>
		<button disabled={savingCertificate}>{config.certificate.loaded ? 'Replace certificate' : 'Upload certificate'}</button>
		<p class="hint">It is stored encrypted and never shown or sent back to the interface.</p>
	</form>
{/if}
