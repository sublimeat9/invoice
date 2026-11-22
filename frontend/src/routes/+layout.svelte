<script lang="ts">
	import favicon from '#lib/assets/favicon.svg';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';
	import './styles.css';

	let { children, data }: LayoutProps = $props();
	const exact = (path: string) => (page.url.pathname === path ? 'page' : undefined);
	const section = (path: string) => (page.url.pathname.startsWith(path) ? 'page' : undefined);
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Electronic invoicing</title>
</svelte:head>

<div class="app">
	<aside class="sidebar">
		<a href="/" class="brand"><span class="logo">F</span> Invoicing</a>
		<nav aria-label="Main">
			<a href="/" aria-current={exact('/')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<path d="M3 2.5h7l3 3v8H3z M10 2.5v3h3 M5.5 8.5h5 M5.5 11h5" />
				</svg>
				Documents
			</a>
			<a href="/invoices/new" aria-current={exact('/invoices/new')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<path d="M8 3v10 M3 8h10" />
				</svg>
				New invoice
			</a>
			<a href="/products" aria-current={section('/products')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<path d="M2.5 5.5 8 2.5l5.5 3v5L8 13.5l-5.5-3z M2.5 5.5 8 8.5l5.5-3 M8 8.5v5" />
				</svg>
				Products
			</a>
			<a href="/customers" aria-current={section('/customers')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<circle cx="8" cy="5.5" r="2.5" /><path d="M3 13c.5-2.5 2.5-4 5-4s4.5 1.5 5 4" />
				</svg>
				Customers
			</a>
			<a href="/settings" aria-current={exact('/settings')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<circle cx="8" cy="8" r="2" /><path d="M8 2v1.5 M8 12.5V14 M2 8h1.5 M12.5 8H14 M3.8 3.8l1 1 M11.2 11.2l1 1 M12.2 3.8l-1 1 M4.8 11.2l-1 1" />
				</svg>
				Settings
			</a>
		</nav>
		{#if data.environment}
			<span class="environment {data.environment}">{data.environment === 'production' ? 'Production' : 'Test'}</span>
		{/if}
	</aside>
	<main class="content">
		{#if data.configReady === false && page.url.pathname !== '/settings'}
			<div class="notice">
				<span>Setup is incomplete: add the issuer details and a certificate before issuing invoices.</span>
				<a class="btn secondary small" href="/settings">Go to settings</a>
			</div>
		{/if}
		{@render children()}
	</main>
</div>
