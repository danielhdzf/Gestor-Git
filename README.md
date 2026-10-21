# Gestor Git

Aplicación de escritorio para Windows que guarda las rutas de tus repositorios locales y ejecuta comandos git en todos ellos, en paralelo o en secuencia repo por repo.

Hecha con [Tauri 2](https://tauri.app/) (backend en Rust) y React + TypeScript.

## Estado

- [x] Paso 1: gestión de repositorios (añadir, buscar en una carpeta, agrupar, quitar) con configuración persistente.
- [x] Paso 2: motor de ejecución en paralelo y en secuencia, con salida en vivo por repo y cancelación.
- [ ] Paso 3: secuencias de comandos guardadas y atajos (status, fetch, pull).
- [ ] Paso 4: instalador de Windows.

## Requisitos (Windows)

- [Node.js](https://nodejs.org/) 20 o superior.
- [Rust](https://www.rust-lang.org/tools/install) con el toolchain MSVC y las "Build Tools for Visual Studio" (C++).
- WebView2 (ya viene con Windows 10/11).
- `git` en el `PATH`.

Más detalles en los [prerrequisitos de Tauri](https://tauri.app/start/prerequisites/).

## Uso en desarrollo

```powershell
npm install
npm run tauri dev
```

Tests del backend:

```powershell
cd src-tauri
cargo test
```

La lista de repositorios se guarda en `%APPDATA%\com.danielhdzf.gestorgit\config.json`.
