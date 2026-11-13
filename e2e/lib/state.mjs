//! Estado compartido entre el runner y las pruebas (sesión + fixture).

let fixture = null;

export function setFixture(f) {
  fixture = f;
}

export function getFixture() {
  if (!fixture) throw new Error('No hay fixture: el runner no lo creó antes de las pruebas.');
  return fixture;
}
