import io, os, re, sys

os.chdir(os.path.dirname(os.path.abspath(__file__)))
import resvg_py
from PIL import Image

# Theme colours from config/files/theme.json (foreground, accent).
VALUES = {"--icon-stroke": "#e6d9db", "--icon-accent": "#f38d70", "--icon-back-opacity": "0.55", "--icon-accent-opacity": "0.45"}
SIZES = (16, 24, 32, 48, 64, 256)


def render(source, size):
    stroke = 6.0 if size <= 24 else 4.5 if size <= 48 else 3.25
    values = {**VALUES, "--icon-stroke-width": str(stroke)}
    svg = re.sub(r"var\((--[\w-]+)[^)]*\)", lambda m: values[m.group(1)], source)
    svg = svg.replace("currentColor", VALUES["--icon-stroke"]).replace("width=\"64\" height=\"64\"", f"width=\"{size}\" height=\"{size}\"")
    return Image.open(io.BytesIO(bytes(resvg_py.svg_to_bytes(svg_string=svg)))).convert("RGBA")


source = open("icon.svg", encoding="utf-8").read()
images = [render(source, size) for size in SIZES]
images[-1].save("icon.ico", sizes=[(size, size) for size in SIZES], append_images=images[:-1])
sheet = Image.new("RGBA", (sum(SIZES) + 20 * len(SIZES), 270), "#2c2525")
x = 10
for image in images:
    sheet.paste(image, (x, 5), image)
    x += image.width + 20
sheet.save(sys.argv[1] if len(sys.argv) > 1 else "preview.png")
