"""Genera el icono de Ariadna en todos los tamaños que usa la app.

Diseño: **corona de Ariadne** (Corona Borealis) en oro, una sola silueta con tres puntas y sus
joyas, y el **hilo** cian que la atraviesa en diagonal. Se dibuja con PIL (sin rsvg/inkscape) y
todo está pensado para leerse a 16 px: formas gruesas, dos colores y márgenes generosos.

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
GOLD_DARK = (206, 150, 40)
CYAN = (34, 211, 238)
CYAN_DARK = (14, 116, 144)


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


def bezier(p0, p1, p2, p3, steps=160):
    """Puntos de una curva cúbica: el hilo no puede tener quiebres."""
    pts = []
    for i in range(steps + 1):
        t = i / steps
        u = 1 - t
        x = u**3 * p0[0] + 3 * u**2 * t * p1[0] + 3 * u * t**2 * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u**2 * t * p1[1] + 3 * u * t**2 * p2[1] + t**3 * p3[1]
        pts.append((x, y))
    return pts


def draw_icon() -> Image.Image:
    img = rounded_bg(S)
    d = ImageDraw.Draw(img)

    # --- Hilo de Ariadna: sale por abajo a la izquierda, cruza por detrás de la corona y termina
    # en un ovillo a la derecha. Trazo grueso (se ve a 16 px) y curva suave.
    thread = bezier((96, 566), (360, 640), (664, 500), (928, 566))
    d.line(thread, fill=CYAN_DARK, width=88, joint="curve")
    d.line(thread, fill=CYAN, width=56, joint="curve")

    # --- Corona: una sola silueta (base + tres puntas y dos valles), centrada.
    crown = [
        (268, 690),
        (268, 560),
        (330, 336),
        (452, 470),
        (512, 258),
        (572, 470),
        (694, 336),
        (756, 560),
        (756, 690),
    ]
    d.polygon(crown, fill=GOLD)
    d.rounded_rectangle([252, 636, 772, 726], radius=34, fill=GOLD)
    d.rounded_rectangle([252, 690, 772, 726], radius=20, fill=GOLD_DARK)
    for jx, jy in [(330, 320), (512, 242), (694, 320)]:
        d.ellipse([jx - 34, jy - 34, jx + 34, jy + 34], fill=GOLD)
        d.ellipse([jx - 12, jy - 12, jx + 12, jy + 12], fill=CYAN)
    # El hilo pasa **por delante** del centro de la corona: así se ve que la atraviesa.
    front = thread[int(len(thread) * 0.44) : int(len(thread) * 0.58)]
    d.line(front, fill=CYAN_DARK, width=84, joint="curve")
    d.line(front, fill=CYAN, width=60, joint="curve")
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
