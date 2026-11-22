import { inTauri } from './api.ts';

/** Saves a text file: native dialog on desktop, a download in the browser. */
export async function saveFile(name: string, content: string, mime: string): Promise<void> {
	if (inTauri()) {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('save_file', { name, content });
		return;
	}
	const url = URL.createObjectURL(new Blob([content], { type: mime }));
	const a = document.createElement('a');
	a.href = url;
	a.download = name;
	a.click();
	URL.revokeObjectURL(url);
}
