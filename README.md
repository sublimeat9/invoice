# invoice-rust

Sistema de facturación electrónica para el SRI de Ecuador. Emite facturas, las firma, las envía
al SRI y guarda su estado de autorización, desde la web, el escritorio o el teléfono.

## Qué incluye

- **Backend (`backend/`)**: escrito en Rust. Genera la clave de acceso y el XML de la factura,
  la firma con XAdES-BES usando el certificado `.p12` del emisor y se comunica con los servicios
  SOAP de recepción y autorización del SRI. Guarda todo en SQLite y expone una API HTTP (axum).
  Reintenta en segundo plano los comprobantes que quedaron pendientes.
- **Interfaz (`frontend/`)**: una aplicación SvelteKit en inglés. En la **web**, un servidor Node
  entrega la interfaz y reenvía las llamadas al backend.
- **Escritorio (`desktop/`)**: la misma interfaz empaquetada con Tauri, con el backend dentro del
  mismo proceso, sin puerto de red; los datos quedan en la carpeta de la aplicación. Los
  instaladores para Windows, macOS y Linux los genera `.github/workflows/desktop.yml`.
- **Android (`android/`)**: cliente nativo en Kotlin y Jetpack Compose que consume la misma API.
  Consulta y filtra comprobantes, reintenta los pendientes, comparte el XML, emite facturas y
  administra productos y clientes.
- **Marca (`brand/`)**: logotipo, paleta y tipografía de la aplicación.

## Cuentas y negocios

Cada cuenta es un negocio independiente: tiene su propio RUC y datos de emisor, su firma
electrónica `.p12`, su ambiente (pruebas o producción), sus productos, clientes, secuenciales y
comprobantes. Nadie ve los datos de otra cuenta.

Cualquier persona puede crearse una cuenta desde la pantalla de acceso (registro público) con su
RUC y una contraseña: el RUC es el usuario. Al registrarse se consulta el RUC en el SRI en línea
(debe existir y estar activo) y el emisor queda precargado con los datos reales: razón social,
nombre comercial, dirección matriz y obligación de llevar contabilidad. La cuenta queda activa
de inmediato y solo falta cargar la firma `.p12` en **Settings**. Si el SRI no responde, el
registro se rechaza en lugar de crear cuentas sin verificar.
La primera cuenta es administradora: en el escritorio es la del propio negocio (se registra con
su RUC), y en un servidor se crea en la primera ejecución o con `ADMIN_USER` y `ADMIN_PASSWORD`: además de su propio negocio, puede ver, desactivar y cambiar el rol
de las demás cuentas, sin acceso a sus datos de facturación. Las instalaciones de una sola empresa
anteriores a las cuentas pasan a ser el negocio de esa primera cuenta.

Las contraseñas se guardan con Argon2. Al iniciar sesión se recibe un token que vence a los 7 días y
se revoca al cerrar sesión, cambiar la contraseña o desactivar la cuenta. Los intentos fallidos se
limitan por usuario y los registros tienen un tope global.

## Seguridad de los datos

El certificado `.p12` se guarda cifrado (AES-256-GCM) con una clave que no está en la base: en el
servidor viene de `ENCRYPTION_KEY`; en el escritorio, del llavero del sistema. El certificado se
carga solo desde la web o el escritorio, nunca desde el teléfono, y cada negocio solo puede usar el suyo. El cliente Android guarda el
token de sesión cifrado con el Keystore y solo permite HTTP sin cifrar hacia el emulador y
`localhost`.

## API

Rutas y claves JSON en inglés (`/invoices`, `/products`, `/customers`, `/config/...`, `/auth/...`,
`/users`), montos en centavos. Los estados del SRI (`AUTORIZADO`, `PENDIENTE`…) y las etiquetas
del XML se conservan tal como las define la ficha técnica.

## Estado

El flujo completo funciona contra el ambiente de pruebas del SRI. Pendientes: RIDE en PDF y envío
por correo, otros tipos de comprobante, varias formas de pago y varios establecimientos.
