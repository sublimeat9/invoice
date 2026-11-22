<script lang="ts">
	import { enhance } from '$app/forms';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();
	let enviando = $state(false);
	let hayPendientes = $derived(data.facturas.some((f) => f.estado === 'PENDIENTE'));
	const clase = (e: string) => e.replaceAll(' ', '_');
</script>

<h1>Facturas</h1>

{#if data.error}
	<div class="panel error">{data.error}</div>
{/if}
{#if form && 'error' in form}
	<div class="panel error">{form.error}</div>
{:else if form && 'reintentadas' in form}
	<div class="panel">Se reintentaron {form.reintentadas} comprobantes pendientes.</div>
{/if}

{#if hayPendientes}
	<form
		method="POST"
		action="?/reintentar"
		use:enhance={() => {
			enviando = true;
			return async ({ update }) => {
				await update();
				enviando = false;
			};
		}}
	>
		<div class="panel">
			Hay facturas pendientes (el SRI no respondió).
			<button class="sec" disabled={enviando}>{enviando ? 'Reintentando…' : 'Reintentar ahora'}</button>
		</div>
	</form>
{/if}

<div class="panel tabla">
	{#if data.facturas.length === 0 && !data.error}
		<p>Aún no hay facturas. <a href="/facturas/nueva">Emite la primera</a>.</p>
	{:else}
		<table>
			<thead>
				<tr><th>N.º</th><th>Estado</th><th>Fecha autorización</th><th>Clave de acceso</th></tr>
			</thead>
			<tbody>
				{#each data.facturas as f (f.clave_acceso)}
					<tr>
						<td>{String(f.secuencial).padStart(9, '0')}</td>
						<td class="estado {clase(f.estado)}">{f.estado}</td>
						<td>{f.fecha_autorizacion ?? '—'}</td>
						<td class="mono"><a href="/facturas/{f.clave_acceso}">{f.clave_acceso}</a></td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>
