import { defineEnvVars } from '@sveltejs/kit/env';

// Variables privadas (solo servidor), leídas al arrancar la app.
export const variables = defineEnvVars({
	API_TOKEN: {
		description: 'Token Bearer del backend; nunca se envía al navegador',
		schema: (value) => {
			if (!value || value.length < 16) throw new Error('API_TOKEN debe tener al menos 16 caracteres');
			return value;
		}
	},
	BACKEND_URL: {
		description: 'URL base del backend Rust',
		schema: (value) => (value ? value.replace(/\/+$/, '') : 'http://127.0.0.1:3000')
	}
});
