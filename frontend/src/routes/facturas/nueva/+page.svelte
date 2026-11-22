<script lang="ts">
	import { enhance } from '$app/forms';
	import { aCentavos, formatear, totales } from '#lib/dinero.ts';
	import type { PageProps } from './$types';

	let { form }: PageProps = $props();

	type L = { codigo: string; descripcion: string; cantidad: string; precio: string; descuento: string; iva: string };
	const vacia = (): L => ({ codigo: '', descripcion: '', cantidad: '1', precio: '', descuento: '', iva: '15' });

	let lineas = $state<L[]>(restaurar() ?? [vacia()]);
	let consumidorFinal = $state(form?.valores?.consumidor_final !== undefined || !form);
	let enviando = $state(false);

	function restaurar(): L[] | null {
		try {
			return form?.valores?.lineas ? JSON.parse(form.valores.lineas) : null;
		} catch {
			return null;
		}
	}

	let resumen = $derived(
		totales(
			lineas.map((l) => ({
				cantidad: Number.isInteger(Number(l.cantidad)) ? Number(l.cantidad) : 0,
				precio: aCentavos(l.precio) ?? 0,
				descuento: aCentavos(l.descuento || '0') ?? 0,
				iva: l.iva
			}))
		)
	);
	const v = (k: string, def = '') => form?.valores?.[k] ?? def;
</script>

<a class="volver" href="/">← Comprobantes</a>
<div class="cabecera"><h1>Nueva factura</h1></div>

{#if form?.error}
	<div class="aviso error">{form.error}</div>
{/if}

<form
	method="POST"
	use:enhance={() => {
		enviando = true;
		return async ({ update }) => {
			await update({ reset: false });
			enviando = false;
		};
	}}
>
	<div class="diseno-form">
	<div>
	<div class="tarjeta">
		<h2>Comprador</h2>
		<p>
			<label class="casilla">
				<input type="checkbox" name="consumidor_final" bind:checked={consumidorFinal} />
				Consumidor final
			</label>
		</p>
		{#if !consumidorFinal}
			<div class="fila">
				<div>
					<label for="tipo">Tipo de identificación</label>
					<select id="tipo" name="tipo_identificacion" value={v('tipo_identificacion', '05')}>
						<option value="05">Cédula</option>
						<option value="04">RUC</option>
						<option value="06">Pasaporte</option>
						<option value="08">Identificación del exterior</option>
					</select>
				</div>
				<div>
					<label for="ident">Identificación</label>
					<input id="ident" name="identificacion" value={v('identificacion')} autocomplete="off" />
				</div>
				<div>
					<label for="rs">Nombre / razón social</label>
					<input id="rs" name="razon_social" value={v('razon_social')} />
				</div>
			</div>
			<div>
				<label for="dir">Dirección (opcional)</label>
				<input id="dir" name="direccion" value={v('direccion')} />
			</div>
		{/if}
		<div class="fila" style="margin-top:.9rem">
			<div>
				<label for="email">Correo (opcional)</label>
				<input id="email" name="email" type="email" value={v('email')} />
			</div>
		</div>
	</div>

	<div class="tarjeta">
		<h2>Detalle</h2>
		{#each lineas as l, i (i)}
			<div class="linea">
			<div class="fila">
				<div><label for="c{i}">Código</label><input id="c{i}" bind:value={l.codigo} placeholder="S/C" /></div>
				<div class="grande">
					<label for="d{i}">Descripción</label><input id="d{i}" bind:value={l.descripcion} />
				</div>
				<div>
					<label for="q{i}">Cantidad</label>
					<input id="q{i}" bind:value={l.cantidad} inputmode="numeric" />
				</div>
				<div>
					<label for="p{i}">Precio unitario</label>
					<input id="p{i}" bind:value={l.precio} inputmode="decimal" placeholder="0.00" />
				</div>
				<div>
					<label for="x{i}">Descuento</label>
					<input id="x{i}" bind:value={l.descuento} inputmode="decimal" placeholder="0.00" />
				</div>
				<div>
					<label for="i{i}">IVA</label>
					<select id="i{i}" bind:value={l.iva}>
						<option value="15">15 %</option>
						<option value="5">5 %</option>
						<option value="0">0 %</option>
						<option value="no_objeto">No objeto</option>
						<option value="exento">Exento</option>
					</select>
				</div>
				<div style="align-self:end">
					{#if lineas.length > 1}
						<button type="button" class="sec chico peligro" onclick={() => lineas.splice(i, 1)}>Quitar</button>
					{/if}
				</div>
			</div>
			</div>
		{/each}
		<button type="button" class="sec" onclick={() => lineas.push(vacia())}>+ Agregar línea</button>
		<input type="hidden" name="lineas" value={JSON.stringify(lineas)} />
	</div>

	<div class="tarjeta">
		<h2>Pago</h2>
		<div class="fila">
			<div>
				<label for="fp">Forma de pago</label>
				<select id="fp" name="forma_pago" value={v('forma_pago', '01')}>
					<option value="01">Sin utilización del sistema financiero</option>
					<option value="16">Tarjeta de débito</option>
					<option value="19">Tarjeta de crédito</option>
					<option value="20">Otros con utilización del sistema financiero</option>
					<option value="17">Dinero electrónico</option>
				</select>
			</div>
		</div>
	</div>
	</div>

	<aside class="tarjeta resumen">
		<h2>Resumen</h2>
		<dl>
			<dt>Subtotal</dt><dd class="num">{formatear(resumen.subtotal)}</dd>
			<dt>IVA</dt><dd class="num">{formatear(resumen.iva)}</dd>
			<dt>Total</dt><dd class="num total">{formatear(resumen.total)}</dd>
		</dl>
		<button disabled={enviando}>{enviando ? 'Enviando al SRI…' : 'Emitir factura'}</button>
	</aside>
	</div>
</form>
