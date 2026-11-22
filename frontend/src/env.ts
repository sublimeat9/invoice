import { defineEnvVars } from '@sveltejs/kit/env';

// Private variables (server only). The desktop build does not use them at all.
export const variables = defineEnvVars({
	API_TOKEN: {
		description: 'Backend Bearer token; never sent to the browser',
		schema: (value) => value ?? ''
	},
	BACKEND_URL: {
		description: 'Base URL of the Rust backend',
		schema: (value) => (value ? value.replace(/\/+$/, '') : 'http://127.0.0.1:3000')
	}
});
