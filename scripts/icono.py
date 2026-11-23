"""Genera el icono de Ariadna en todos los tamaños que usa la app.

Diseño: monograma **«A» hecho con el hilo de Ariadna** (cian) y **nodos de commit** (oro): el hilo
entra por el anillo de abajo a la izquierda, sube al vértice, baja y sale por el nodo dorado; la
barra es una rama. Es el mismo dibujo que `ui/src/lib/components/Logo.svelte` (cuadrícula de 32).
Se dibuja con PIL (sin rsvg/inkscape) y está pensado para leerse a 16 px: trazos gruesos, tres
colores y márgenes generosos.

Requisitos: **Pillow** (`python3 -m pip install pillow`); es lo único que hace falta, y no hay
dependencia de ImageMagick, rsvg ni de las herramientas de Tauri.

Si el ICNS no se puede escribir, el script termina con código 1 para que nadie publique un
`icon.icns` viejo creyendo que se regeneró.
"""
import os
from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src-tauri", "icons")
UI = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ui", "static")
S = 1024

GOLD = (247, 197, 84)
CYAN = (34, 211, 238)
NIGHT = (11, 24, 38)


def rounded_bg(size: int) -> Image.Image:
    """Fondo con degradado vertical suave y esquinas redondeadas."""
    top, bottom = (14, 29, 45), (7, 15, 24)
    grad = Image.new("RGBA", (size, size))
    gd = ImageDraw.Draw(grad)
    for y in range(size):
        t = y / max(1, size - 1)
        gd.line(
            [(0, y), (size, y)],
            fill=tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,),
        )
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=round(size * 0.22), fill=255)
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    img.paste(grad, (0, 0), mask)
    return img


K = S / 32  # el dibujo se define en una cuadrícula de 32 (como el SVG) y se escala


def stroke(d: ImageDraw.ImageDraw, pts, width, fill):
    """Polilínea de trazo grueso con extremos y uniones redondos."""
    px = [(x * K, y * K) for x, y in pts]
    d.line(px, fill=fill, width=round(width * K), joint="curve")
    r = width * K / 2
    for x, y in px:
        d.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def node(d: ImageDraw.ImageDraw, cx, cy, r, fill, hole=None, ring=None):
    """Nodo de commit: disco, con agujero oscuro (`hole`) o como anillo (`ring` = grosor)."""
    x, y = cx * K, cy * K
    if ring:  # como un trazo SVG: `r` es el radio medio y `ring` el grosor
        r += ring / 2
    rr = r * K
    d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=fill)
    if hole:
        h = hole * K
        d.ellipse([x - h, y - h, x + h, y + h], fill=NIGHT)
    if ring:
        h = (r - ring) * K
        d.ellipse([x - h, y - h, x + h, y + h], fill=NIGHT)


def draw_icon() -> Image.Image:
    img = rounded_bg(S)
    d = ImageDraw.Draw(img)
    # El vértice es una curva cuadrática: se muestrea para que no haya quiebre.
    apex = []
    for i in range(0, 21):
        t = i / 20
        u = 1 - t
        apex.append((u * u * 14.1 + 2 * u * t * 16 + t * t * 17.9, u * u * 9.3 + 2 * u * t * 6 + t * t * 9.3))
    stroke(d, [(3.5, 27)] + apex + [(28.5, 27)], 2.9, CYAN)
    stroke(d, [(10, 19), (22, 19)], 2.4, CYAN)
    node(d, 16, 7.4, 3, GOLD, hole=1.1)
    node(d, 10, 19, 1.8, GOLD)
    node(d, 22, 19, 1.8, GOLD)
    node(d, 3.9, 26.4, 2, CYAN, ring=1.5)
    node(d, 28.1, 26.4, 2.3, GOLD)
    return img


def main() -> int:
    """Escribe todos los Tamaños; devuelve 0 solo si el ICNS también se escribió."""
    big = draw_icon()
    big.save(os.path.join(OUT, "source-1024.png"))
    big.save(os.path.join(OUT, "icon.png"))
    for size, name in [(32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png")]:
        big.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, name))
    big.save(
        os.path.join(OUT, "icon.ico"),
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    icns_ok = True
    try:
        big.save(
            os.path.join(OUT, "icon.icns"),
            sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)],
        )
    except Exception as exc:  # noqa: BLE001
        # No se deja un `icon.icns` viejo por ahí: o se regenera o el script falla.
        print(f"ERROR: no se pudo escribir icon.icns ({exc}); el que hay en disco es el viejo")
        icns_ok = False
    os.makedirs(UI, exist_ok=True)
    big.resize((512, 512), Image.LANCZOS).save(os.path.join(UI, "icon-512.png"))
    big.resize((64, 64), Image.LANCZOS).save(os.path.join(UI, "icon-64.png"))
    if not icns_ok:
        print("icono incompleto: falta el ICNS de macOS")
        return 1
    print("icono generado")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
