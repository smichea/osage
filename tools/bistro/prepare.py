# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Makes Amazon Lumberyard's Bistro ready to photograph and to walk in (D-134).

    blender -b --factory-startup --python tools/bistro/prepare.py -- BISTRO OUT [--render FILE]

BISTRO is the unzipped `Bistro_v5_2` folder of the asset manifest's
`assets/orca/bistro/Bistro_v5_2.zip` (NVIDIA ORCA, CC BY 4.0); OUT receives
`exterior.blend`, the street ready for Blender's path tracer (Cycles):

- the FBX's analytic lights left out (its readme: a ray tracer lights the
  street from its sky and its emissive surfaces);
- each material rebuilt from its textures, as their readme describes them:
  base colour with opacity in alpha, "Specular" holding occlusion, roughness
  and metalness in red, green and blue, normals in DirectX's convention
  (green turned over for Blender's), emissive colour where there is one;
  glass transmissive;
- the sky from the HDR photograph that comes with it (Poly Haven's San
  Giuseppe Bridge, CC0).

With `--render FILE`, a picture from the FBX's own camera is rendered there
too, to look at the result. Runs inside Blender (tested with 5.2).
"""

import pathlib
import sys
import time

import bpy

EMISSION = 6.0
SAMPLES = 64
# The sky's strength, as the scene's Falcor file sets it.
SKY = 10.0


def args():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if len(rest) < 2:
        sys.exit(__doc__)
    render = None
    if "--render" in rest:
        render = pathlib.Path(rest[rest.index("--render") + 1]).resolve()
    return pathlib.Path(rest[0]).resolve(), pathlib.Path(rest[1]).resolve(), render


def textures_of(material):
    """The material's textures by kind, from the images the importer linked."""
    found = {}
    if material.node_tree is None:
        return found
    for node in material.node_tree.nodes:
        if node.bl_idname == "ShaderNodeTexImage" and node.image is not None:
            name = node.image.name.lower()
            for kind in ("basecolor", "specular", "normal", "emissive"):
                if f"_{kind}" in name:
                    found[kind] = node.image
    return found


def emissive_image(material, folder):
    """An emissive texture the importer did not link, by the material's name."""
    for suffix in (".dds", ".tga", ".TGA"):
        path = folder / "Textures" / f"{material.name}_Emissive{suffix}"
        if path.exists():
            return bpy.data.images.load(str(path), check_existing=True)
    return None


def rebuild(material, folder):
    textures = textures_of(material)
    if "emissive" not in textures:
        image = emissive_image(material, folder)
        if image is not None:
            textures["emissive"] = image
    tree = material.node_tree
    tree.nodes.clear()
    nodes, links = tree.nodes, tree.links
    out = nodes.new("ShaderNodeOutputMaterial")
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])

    def texture(image, colour):
        node = nodes.new("ShaderNodeTexImage")
        node.image = image
        image.colorspace_settings.name = "sRGB" if colour else "Non-Color"
        return node

    if "basecolor" in textures:
        base = texture(textures["basecolor"], True)
        links.new(base.outputs["Color"], bsdf.inputs["Base Color"])
        if textures["basecolor"].channels == 4:
            links.new(base.outputs["Alpha"], bsdf.inputs["Alpha"])
    if "specular" in textures:
        spec = texture(textures["specular"], False)
        split = nodes.new("ShaderNodeSeparateColor")
        links.new(spec.outputs["Color"], split.inputs["Color"])
        links.new(split.outputs["Green"], bsdf.inputs["Roughness"])
        links.new(split.outputs["Blue"], bsdf.inputs["Metallic"])
    if "normal" in textures:
        normal = texture(textures["normal"], False)
        split = nodes.new("ShaderNodeSeparateColor")
        links.new(normal.outputs["Color"], split.inputs["Color"])
        # DirectX's green points down; Blender's up.
        turn = nodes.new("ShaderNodeMath")
        turn.operation = "SUBTRACT"
        turn.inputs[0].default_value = 1.0
        links.new(split.outputs["Green"], turn.inputs[1])
        join = nodes.new("ShaderNodeCombineColor")
        links.new(split.outputs["Red"], join.inputs["Red"])
        links.new(turn.outputs["Value"], join.inputs["Green"])
        links.new(split.outputs["Blue"], join.inputs["Blue"])
        mapping = nodes.new("ShaderNodeNormalMap")
        links.new(join.outputs["Color"], mapping.inputs["Color"])
        links.new(mapping.outputs["Normal"], bsdf.inputs["Normal"])
    if "emissive" in textures:
        glow = texture(textures["emissive"], True)
        links.new(glow.outputs["Color"], bsdf.inputs["Emission Color"])
        bsdf.inputs["Emission Strength"].default_value = EMISSION
    if "glass" in material.name.lower():
        bsdf.inputs["Transmission Weight"].default_value = 1.0
        bsdf.inputs["IOR"].default_value = 1.5
        if "specular" not in textures:
            bsdf.inputs["Roughness"].default_value = 0.05
        if "frosted" in material.name.lower():
            bsdf.inputs["Roughness"].default_value = 0.35


def sky(folder):
    world = bpy.data.worlds.new("Bistro sky")
    bpy.context.scene.world = world
    tree = world.node_tree
    tree.nodes.clear()
    environment = tree.nodes.new("ShaderNodeTexEnvironment")
    environment.image = bpy.data.images.load(str(folder / "san_giuseppe_bridge_4k.hdr"))
    background = tree.nodes.new("ShaderNodeBackground")
    background.inputs["Strength"].default_value = SKY
    out = tree.nodes.new("ShaderNodeOutputWorld")
    tree.links.new(environment.outputs["Color"], background.inputs["Color"])
    tree.links.new(background.outputs["Background"], out.inputs["Surface"])


def main():
    folder, out, render = args()
    out.mkdir(parents=True, exist_ok=True)
    started = time.time()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.wm.fbx_import(filepath=str(folder / "BistroExterior.fbx"))
    for obj in [o for o in bpy.data.objects if o.type == "LIGHT"]:
        bpy.data.objects.remove(obj, do_unlink=True)
    for material in bpy.data.materials:
        rebuild(material, folder)
    sky(folder)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = SAMPLES
    scene.cycles.use_denoising = True
    scene.cycles.max_bounces = 8
    scene.cycles.transparent_max_bounces = 24
    scene.cycles.sample_clamp_indirect = 10.0
    cameras = [o for o in bpy.data.objects if o.type == "CAMERA"]
    if cameras:
        scene.camera = cameras[0]
    scene.frame_set(1)
    bpy.ops.wm.save_as_mainfile(filepath=str(out / "exterior.blend"), relative_remap=True)
    print(f"bistro: prepared in {time.time() - started:.0f} s: {out / 'exterior.blend'}", flush=True)
    if render is not None:
        scene.render.resolution_x, scene.render.resolution_y = 960, 540
        scene.render.filepath = str(render)
        started = time.time()
        bpy.ops.render.render(write_still=True)
        print(f"bistro: rendered in {time.time() - started:.0f} s: {render}", flush=True)


main()
