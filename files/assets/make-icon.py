import io, os, re, sys

os.chdir(os.path.dirname(os.path.abspath(__file__)))
import resvg_py
from PIL import Image

# Theme colours from config/files/theme.json (foreground, accent).
VALUES = {"--icon-stroke": "#d4d4d8", "--icon-accent": "#8b919c", "--icon-back-opacity": "0.55", "--icon-accent-opacity": "0.45"}
SIZES = (16, 24, 32, 48, 64, 256)


def render(source, size):
    stroke = 6.0 if size <= 24 else 4.5 if size <= 48 else 3.25
    values = {**VALUES, "--icon-stroke-width": str(stroke)}
    svg = re.sub(r"var\((--[\w-]+)[^)]*\)", lambda m: values[m.group(1)], source)
    svg = svg.replace("currentColor", VALUES["--icon-stroke"]).replace("width=\"64\" height=\"64\"", f"width=\"{size}\" height=\"{size}\"").replace("viewBox=\"0 0 64 64\"", "viewBox=\"10 7 48 48\"")
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


# Alpha masks per layer (back folder, accent panel, front outline + dash) so the app can tint them with the theme.
LAYER_SIZES = (16, 24, 32, 48)
LAYER_KEEP = {"back": ('class="back',), "accent": ('class="accent',), "front": ('class="front', 'class="dash')}


def layer_mask(layer, size):
    keep = LAYER_KEEP[layer]
    lines = [line for line in source.splitlines() if not re.search(r'class="(back|accent|front|dash)', line) or any(k in line for k in keep)]
    white = {"--icon-stroke": "#ffffff", "--icon-accent": "#ffffff", "--icon-back-opacity": "1", "--icon-accent-opacity": "1"}
    values = {**white, "--icon-stroke-width": str(6.0 if size <= 24 else 4.5)}
    svg = re.sub(r"var\((--[\w-]+)[^)]*\)", lambda m: values[m.group(1)], "\n".join(lines))
    svg = svg.replace("currentColor", "#ffffff").replace('width="64" height="64"', f'width="{size}" height="{size}"')
    return Image.open(io.BytesIO(bytes(resvg_py.svg_to_bytes(svg_string=svg)))).convert("RGBA").getchannel("A").tobytes()


with open("icon-layers.bin", "wb") as out:
    for size in LAYER_SIZES:
        for layer in ("back", "accent", "front"):
            out.write(layer_mask(layer, size))
