# invoice-rust

Facturación electrónica para el SRI de Ecuador.

- `backend/`: lógica en Rust (clave de acceso, XML, firma XAdES-BES, cliente SOAP del SRI,
  persistencia SQLite). Ver `backend/examples/` para ejemplos de uso.
- `frontend/`: cliente SvelteKit que consumirá la API del backend (pendiente).

```
cd backend && cargo test
```
