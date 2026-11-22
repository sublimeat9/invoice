import adapterNode from '@sveltejs/adapter-node';
import adapterStatic from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// `vite build --mode tauri` produces the static bundle embedded in the desktop app;
// any other mode produces the Node server for the web (with the `/api` proxy).
export default defineConfig(({ mode }) => ({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: mode === 'tauri' ? adapterStatic({ fallback: 'index.html' }) : adapterNode()
		})
	],
	// Tauri expects a fixed port in dev mode.
	server: mode === 'tauri' ? { port: 1420, strictPort: true } : undefined
}));
