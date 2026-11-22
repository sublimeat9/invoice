import { defineEnvVars } from '@sveltejs/kit/env';

// Private variables (server only). The desktop build does not use them at all.
export const variables = defineEnvVars({
	BACKEND_URL: {
		description: 'Base URL of the Rust backend',
		schema: (value) => (value ? value.replace(/\/+$/, '') : 'http://127.0.0.1:3000')
	}
});
