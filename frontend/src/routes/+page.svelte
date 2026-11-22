<script lang="ts">
	import { enhance } from '$app/forms';
	import { formatear } from '#lib/dinero.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();
	let enviando = $state(false);
	let hayPendientes = $derived(data.facturas.some((f) => f.estado === 'PENDIENTE'));
	const clase = (e: string) => e.replaceAll(' ', '_');
	const TIPOS: Record<string, string> = { '01': 'Factura' };

	/** Enlace conservando los filtros actuales. */
	function enlace(base: string, extra: Record<string, string | number> = {}): string {
		const p = new URLSearchParams(data.filtros as Record<string, string>);
		for (const [k, v] of Object.entries(extra)) p.set(k, String(v));
		const s = p.toString();
		return s ? `${base}?${s}` : base;
	}
	const f = (k: string) => (data.filtros as Record<string, string>)[k] ?? '';
</script>

<div class="cabecera">
	<div>
		<h1>Comprobantes</h1>
		<p>Consulta y descarga las facturas emitidas.</p>
	</div>
	<a class="boton" href="/facturas/nueva">Nueva factura</a>
</div>

<form method="GET" class="tarjeta">
	<div class="fila">
		<div><label for="desde">Fecha inicio</label><input id="desde" name="desde" type="date" value={f('desde')} /></div>
		<div><label for="hasta">Fecha fin</label><input id="hasta" name="hasta" type="date" value={f('hasta')} /></div>
		<div>
			<label for="ident">Identificación</label>
			<input id="ident" name="identificacion" value={f('identificacion')} autocomplete="off" />
		</div>
		<div>
			<label for="clave">Clave / autorización</label>
			<input id="clave" name="clave" value={f('clave')} autocomplete="off" />
		</div>
		<div>
			<label for="estado">Estado</label>
			<select id="estado" name="estado" value={f('estado')}>
				<option value="">Todos</option>
				<option>AUTORIZADO</option>
				<option>PENDIENTE</option>
				<option>RECHAZADO</option>
				<option>DEVUELTA</option>
				<option value="NO AUTORIZADO">NO AUTORIZADO</option>
			</select>
		</div>
	</div>
	<div class="acciones">
		<button>Buscar</button>
		<a href="/" class="boton sec">Limpiar</a>
	</div>
</form>

{#if data.error}
	<div class="aviso error">{data.error}</div>
{/if}
{#if form && 'error' in form}
	<div class="aviso error">{form.error}</div>
{:else if form && 'reintentadas' in form}
	<div class="aviso info">Se reintentaron {form.reintentadas} comprobantes pendientes.</div>
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
		<div class="aviso">
			<span>Hay facturas pendientes: el SRI no respondió.</span>
			<button class="sec chico" disabled={enviando}>{enviando ? 'Reintentando…' : 'Reintentar ahora'}</button>
		</div>
	</form>
{/if}

<div class="tarjeta sin-relleno tabla">
	{#if data.facturas.length === 0 && !data.error}
		<div class="vacio">No se encontraron resultados.</div>
	{:else}
		<table>
			<thead>
				<tr>
					<th>Fecha</th><th>Tipo</th><th>Número</th><th>Identificación</th>
					<th>Razón social</th><th>Autorización</th><th>Estado</th>
					<th class="der">Total</th>
				</tr>
			</thead>
			<tbody>
				{#each data.facturas as r (r.clave_acceso)}
					<tr>
						<td>{r.fecha_emision ?? '—'}</td>
						<td>{TIPOS[r.tipo_comprobante] ?? r.tipo_comprobante}</td>
						<td>{r.numero}</td>
						<td>{r.identificacion ?? '—'}</td>
						<td class="truncar">{r.razon_social ?? '—'}</td>
						<td class="mono truncar"><a href="/facturas/{r.clave_acceso}" title={r.numero_autorizacion ?? r.clave_acceso}>{r.numero_autorizacion ?? r.clave_acceso}</a></td>
						<td><span class="etiqueta {clase(r.estado)}">{r.estado}</span></td>
						<td class="der num">{r.valor_total === null ? '—' : formatear(r.valor_total)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<div class="pie-tabla">
			<span>{data.total} resultados · página {data.pagina} de {data.paginas}</span>
			<span class="acciones">
				{#if data.pagina > 1}<a class="boton sec chico" href={enlace('/', { pagina: data.pagina - 1 })}>Anterior</a>{/if}
				{#if data.pagina < data.paginas}<a class="boton sec chico" href={enlace('/', { pagina: data.pagina + 1 })}>Siguiente</a>{/if}
				<a class="boton sec chico" href={enlace('/reporte.csv')} download>Descargar CSV</a>
			</span>
		</div>
	{/if}
</div>

