#!/usr/bin/env bash
#
# Pruebas de `rust-gate.sh`: los tres estados del gate (ok / fallo real / sin runner) y el paso de
# «quedan reintentos» a «sin runner tras N intentos». Sin red: los jobs son fixtures en JSON.
#
# Uso: bash scripts/ci/rust-gate.test.sh   (necesita bash y jq; lo ejecuta también el CI)

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
gate="$here/rust-gate.sh"
# Directorio temporal **dentro del repo**: jq es nativo y no entiende las rutas de MSYS.
tmp="$PWD/.rust-gate-test.$$"
mkdir -p "$tmp"
trap 'rm -rf "$tmp"' EXIT

fallos=0

# jobs.json con los jobs de un run; recibe pares nombre=conclusión.
jobs_json() {
  local first=1
  printf '{"jobs":['
  local i=100
  for kv in "$@"; do
    local name="${kv%%=*}" concl="${kv##*=}"
    [ "$first" -eq 1 ] || printf ','
    first=0
    i=$((i + 1))
    printf '{"id":%d,"name":"%s","conclusion":"%s"}' "$i" "$name" "$concl"
  done
  printf ']}'
}

falla() {
  echo "FALLO: $1"
  fallos=$((fallos + 1))
}

# Ejecuta el gate y deja el código en $rc y la salida en $out. Argumentos: [intento] [no-runner].
run_gate() {
  local attempt="${1:-1}" norunner="${2:-}"
  local args=(--jobs "$tmp/jobs.json" --attempt "$attempt" --max 3)
  [ -n "$norunner" ] && args+=(--no-runner "$norunner")
  out=$(bash "$gate" "${args[@]}" 2>&1)
  rc=$?
}

SIN_RUNNER_FIXTURE=(
  "test (macos-15)=cancelled"
  "test (ubuntu-24.04)=success"
  "test (windows-2025)=success"
  "fmt + clippy (ubuntu)=success"
  "gitleaks=success"
  "rust-ok=failure"
)

# 1) Todo verde → 0.
jobs_json "test (ubuntu-24.04)=success" "test (macos-15)=success" "test (windows-2025)=success" \
  "fmt + clippy (ubuntu)=success" "gitleaks=success" "rust-ok=success" "test (windows-11-arm, mejor esfuerzo)=success" > "$tmp/jobs.json"
run_gate
[ "$rc" = 0 ] || falla "todo verde debería salir 0 (salió $rc)"
case "$out" in *"todo correcto"*) ;; *) falla "todo verde debería decirlo en el resumen" ;; esac

# 2) Un test roto de verdad → 1, aunque el resto esté bien.
jobs_json "test (ubuntu-24.04)=failure" "test (macos-15)=success" "test (windows-2025)=success" \
  "fmt + clippy (ubuntu)=success" "gitleaks=success" "rust-ok=failure" > "$tmp/jobs.json"
run_gate
[ "$rc" = 1 ] || falla "un test roto debería salir 1 (salió $rc)"
case "$out" in *"fallo real"*) ;; *) falla "un test roto debería decir «fallo real»" ;; esac
# El job de ARM (mejor esfuerzo) no debe convertir un verde en rojo.
jobs_json "test (ubuntu-24.04)=success" "test (macos-15)=success" "test (windows-2025)=success" \
  "fmt + clippy (ubuntu)=success" "gitleaks=success" "rust-ok=success" \
  "test (windows-11-arm, mejor esfuerzo)=failure" > "$tmp/jobs.json"
run_gate
[ "$rc" = 0 ] || falla "«mejor esfuerzo» en rojo no debería tumbar el gate (salió $rc)"

# 3) Sin runner con reintentos pendientes → 3 (el workflow lo convierte en rojo y relanza).
jobs_json "${SIN_RUNNER_FIXTURE[@]}" > "$tmp/jobs.json"
# En este fixture el primer job es macOS, así que su id es 101 (jobs_json empieza en 101).
printf '101\n' > "$tmp/no-runner.txt"
run_gate 1 "$tmp/no-runner.txt"
[ "$rc" = 3 ] || falla "sin runner con intentos pendientes debería salir 3 (salió $rc)"
case "$out" in *"se relanzará"*) ;; *) falla "debería anunciar el relanzamiento" ;; esac

# 4) Sin runner y sin reintentos → 0 con aviso «sin runner tras 3 intentos».
run_gate 3 "$tmp/no-runner.txt"
[ "$rc" = 0 ] || falla "sin runner en el último intento debería salir 0 (salió $rc)"
case "$out" in *"sin runner tras 3 intentos"*) ;; *) falla "debería decir «sin runner tras 3 intentos»: $out" ;; esac

# 5) Un cancelado que NO es por falta de runner se trata como fallo real (podría tapar algo).
jobs_json "test (ubuntu-24.04)=success" "test (macos-15)=cancelled" "test (windows-2025)=success" \
  "fmt + clippy (ubuntu)=success" "gitleaks=success" "rust-ok=failure" > "$tmp/jobs.json"
printf '' > "$tmp/no-runner-vacio.txt"
run_gate 1 "$tmp/no-runner-vacio.txt"
[ "$rc" = 1 ] || falla "un cancelado sin anotación de runner debería ser fallo real (salió $rc)"

# 6) Falta runner y además hay un test roto → manda el fallo real (1), no el relanzamiento.
jobs_json "test (ubuntu-24.04)=failure" "test (macos-15)=cancelled" "test (windows-2025)=success" \
  "fmt + clippy (ubuntu)=success" "gitleaks=success" "rust-ok=failure" > "$tmp/jobs.json"
run_gate 1 "$tmp/no-runner.txt"
[ "$rc" = 1 ] || falla "con un test roto no se relanza: debería salir 1 (salió $rc)"

if [ "$fallos" -gt 0 ]; then
  echo "== $fallos comprobación(es) fallida(s)"
  exit 1
fi
echo "== rust-gate: todas las comprobaciones en verde"
