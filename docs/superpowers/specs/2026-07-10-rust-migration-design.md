# Migración de caelestia-cli a Rust — Diseño

**Fecha:** 2026-07-10
**Estado:** Aprobado
**Decisiones:** Full Rust incremental · Drop-in 100% compatible · Fork NixOS-only · `install`/`update` eliminados (stub)

## 1. Contexto y motivación

caelestia-cli es ~5,200 líneas de Python 3.13. Tres capas:

1. **Lógica de color (~900 líneas):** `material/`, `theme.py`, `colour.py`, `colourfulness.py`, `scheme.py`. Depende de `materialyoucolor` (HCT, QuantizerCelebi, Score, 9 variantes de scheme, Blend) y `Pillow` (thumbnails, pixel access, primer frame de GIF).
2. **Orquestación de binarios (~60% del código):** subprocess a ~30 binarios externos (grim, slurp, wl-copy, cliphist, fuzzel, gpu-screen-recorder, ffmpeg, dart-sass, notify-send, git, qs, dconf, etc.).
3. **IPC Hyprland:** sockets Unix crudos — `.socket.sock` (request/response, prefijo `j/`, `[[BATCH]]`) y `.socket2.sock` (stream de eventos para el daemon resizer).

Motivación: latencia de arranque. El intérprete Python + imports cuestan ~150-200ms por invocación, y los keybinds de Hyprland invocan el CLI constantemente. Rust: ~5ms, binario único, sin venv.

### Por qué NO híbrido PyO3

- El beneficio principal (arranque) se pierde si Python sigue siendo el entry point.
- Rust embebiendo Python (pyo3) = complejidad de empaquetado enorme sin ganancia: no existe funcionalidad Python-only.
- Solo hay 2 dependencias third-party y ambas tienen equivalente Rust directo y verificado.

### Equivalencias verificadas

| Python | Rust | Cobertura |
|---|---|---|
| `materialyoucolor` | crate `material-colors` v0.4.2 | 100%: 9 variantes de scheme, QuantizerCelebi, Score, Blend, DislikeAnalyzer, HCT, MaterialDynamicColors (verificado contra el árbol de fuentes del crate) |
| `Pillow` | crate `image` v0.25 | thumbnail, convert RGB, JPEG/PNG, primer frame GIF |
| `argparse` | `clap` v4 (derive) | directo |
| `socket`, `subprocess`, `json`, `tomllib`, `fcntl` | `std::os::unix::net`, `std::process::Command`, `serde_json`, `toml`, `rustix::fs::flock` | stdlib/crates estándar |

Cero `eval`/reflection real en el código Python. Los trucos dinámicos (`getattr`, mapa de lambdas del dispatcher Lua en `hypr.py`, decorators) se portan como `match`/closures.

## 2. Alcance

**Se porta (12 → 8 subcomandos nativos):** shell, toggle, scheme, screenshot, record, wallpaper, resizer, search.

**No se porta:** `install` y `update` (~1,400 líneas de lógica pacman/paru/pkgit/AUR — sin sentido en NixOS). Quedan como stubs: imprimen "NixOS fork — gestión vía flake" y salen con código 1. Los módulos `utils/dots/` mueren con ellos.

**Se elimina (decisión 2026-07-10, análisis de overlap con el shell):** `clipboard` y `emoji`. El shell (fork en `~/Hobby/shell`, migrando QML→C++) ya los reimplementa por completo en su launcher (`ClipboardCore`/`EmojisCore` en C++, favoritos, previews) sin depender de fuzzel. Las versiones CLI eran pickers fuzzel inferiores. Quedan como stubs: "handled by the shell launcher" + exit 1.

### 2.1 Contrato congelado CLI ↔ shell

Criterio de frontera: UI/interactivo vive en el shell; headless/estado/captura/daemon vive en el CLI. El shell ejecuta el binario `caelestia` y lee sus archivos de estado — el port Rust NO puede cambiar:

- Archivos: `~/.local/state/caelestia/scheme.json` (formato JSON completo) y `~/.local/state/caelestia/wallpaper/path.txt` (el shell los observa vía FileView).
- Invocaciones exactas del shell (services/Wallpapers.qml, Colours.qml, Recorder.qml): `caelestia wallpaper -f <path> [--no-smart]`, `caelestia wallpaper -r [--no-smart]`, `caelestia wallpaper -p <path> [--no-smart]`, `caelestia scheme set --notify -m <mode>`, `caelestia record`, `caelestia record -p`, `caelestia record <args>`.
- Del lado CLI→shell: `qs -c caelestia ipc call picker openFreeze/openFreezeClip/openSearch` (screenshot/search).

## 3. Arquitectura

Crate único `caelestia` (binario). Módulos espejan el layout Python para facilitar port 1:1 y review:

```
caelestia-cli/
├── flake.nix          # devshell (rust + python/uv para golden tests) + package
├── Cargo.toml
├── src/
│   ├── main.rs        # entry + dispatch + manejo de error global
│   ├── cli.rs         # clap derive — árbol completo de subcomandos, flags idénticos
│   ├── subcommands/   # shell, toggle, scheme, screenshot, record, clipboard,
│   │                  # emoji, wallpaper, resizer, search, install(stub), update(stub)
│   ├── core/
│   │   ├── material/  # score.rs, generator.rs (gen_scheme, harmonize, mix, lighten…)
│   │   ├── scheme.rs  # modelo Scheme, load/save JSON — mismo formato que Python
│   │   ├── theme.rs   # apply_colours + templating {{ $var }} / {{ col.form }} / {{ $mode }}
│   │   ├── wallpaper.rs
│   │   └── colour.rs  # Colour (hex/rgb/rgba) + colourfulness (Hasler-Süsstrunk)
│   ├── ipc/hypr.rs    # .socket.sock (j/, [[BATCH]]) + .socket2.sock (eventos)
│   └── util/          # paths (XDG), io (log/warn/fatal, prompts), notify, exec helpers
├── python-ref/        # código Python actual movido — referencia + golden tests
└── tests/golden/      # harness de comparación Rust vs Python
```

### Dependencias Cargo

- `clap` (derive) — parser; help y flags idénticos al argparse actual.
- `material-colors` — motor de color.
- `image` — thumbnails, GIF, pixel access.
- `serde` / `serde_json` — schemes, caches, respuestas IPC de Hyprland.
- `toml` — solo si algún resto lo necesita.
- `anyhow` — errores con contexto.
- `rustix` (o `fs2`) — flock para el lock de theme.

## 4. Compatibilidad drop-in (contrato)

- Mismos paths XDG: `~/.local/state/caelestia/`, `~/.cache/caelestia/`, `~/.config/caelestia/`.
- Mismo formato JSON de scheme y de caches — los caches existentes siguen siendo válidos (mismo hash de wallpaper: replicar `compute_hash` exacto).
- Mismos nombres de subcomandos, flags, defaults, exit codes y mensajes que consume caelestia-shell.
- Fish completions actuales funcionan sin cambios.
- Hook `postHook` de wallpaper: mismas variables de entorno (`WALLPAPER_PATH`, `SCHEME_*`, `THUMBNAIL_PATH`), ejecutado con `sh -c`.
- Hooks de manifest y `shell=True` en general → `sh -c` en Rust.

## 5. Estrategia incremental — delegación

Durante la migración el binario Rust es el entry point y despacha subcomandos aún no portados al código Python (`python -m caelestia <args>` contra `python-ref/`). El sistema queda drop-in funcional desde la fase 1; cada fase mueve subcomandos a nativo. Al terminar, la delegación y `python-ref/` se eliminan.

## 6. Golden tests (riesgo principal: fidelidad de color)

`material-colors` (Rust) y `materialyoucolor` (Python) son ports independientes de material-color-utilities; pueden diferir en redondeo o versión de spec.

Harness en `tests/golden/`: para N wallpapers de muestra × 9 variantes × modos × flavours, genera el JSON de colores con Python y con Rust y hace diff. Tolerancia inicial: exacta; si hay diferencias de redondeo, se documenta y se acepta ±1 por canal RGB con evidencia. El devshell del flake incluye python+uv solo mientras dure la migración.

Nota: el crate expone `color_spec_2021/2025/2026`; se fija el spec que reproduzca el output de `materialyoucolor` actual (esperado: 2021).

## 7. Fases

1. **Scaffolding** — Cargo + clap con el árbol completo + delegación a Python + flake.nix nuevo (rustPlatform.buildRustPackage + devshell con rust toolchain y uv). El binario ya reemplaza al actual.
2. **Util core + subcomandos triviales** — `util/` (paths, io, notify), `ipc/hypr.rs`, `colour.rs`. Nativos: toggle, shell, screenshot, record, search. Stubs: clipboard, emoji (reemplazados por el launcher del shell).
3. **Motor de color** — `material/score.rs`, `material/generator.rs`, `scheme.rs` + golden tests pasando.
4. **Theme + wallpaper** — templating, `apply_colours`, `set_wallpaper` (incl. frame de video vía ffmpeg, GIF, symlinks, smart mode). Nativos: scheme, wallpaper.
5. **Resizer daemon** — event loop sobre `.socket2.sock`, matching de ventanas, dispatch (incl. variante Lua).
6. **Limpieza** — stubs install/update, borrar `python-ref/` y delegación, README con lista de binarios runtime (grim, slurp, swappy, wl-clipboard, gpu-screen-recorder, ffmpeg, dart-sass, libnotify, dconf, killall, xdg-utils, curl, qs/caelestia-shell; cliphist/fuzzel salen — eran solo para clipboard/emoji), flake final sin Python, borrar `bin/` y `bldit.lua` legacy.

## 8. Manejo de errores

- `anyhow::Result` en toda la cadena; `main` captura, imprime a stderr con el mismo estilo de `utils/io.py` (colores ANSI), exit 1.
- Binario externo ausente → error claro con nombre del binario (hoy Python lanza FileNotFoundError críptico — se mantiene exit code, se mejora mensaje).
- Errores de IPC Hyprland (socket ausente = no estás en Hyprland) → mensaje explícito.

## 9. Testing

- Unit tests: colour conversions, colourfulness, templating, parser del protocolo IPC, compute_hash.
- Golden tests de color (sección 6).
- Smoke manual en Hyprland real por fase (la máquina del usuario es el entorno target).
- `cargo clippy` + `cargo fmt` en devshell.

## 10. Empaquetado Nix

- `flake.nix`: package via `rustPlatform.buildRustPackage`, binarios runtime inyectados (wrapper PATH o propagación, como el default.nix actual) y documentados en README.
- Devshell: rust toolchain completo + (temporal) python 3.13 + uv para golden tests.
- Se elimina `default.nix` basado en buildPythonApplication al final de la fase 6; los patches del patchPhase actual (qs→caelestia-shell) se resuelven en código o config.
