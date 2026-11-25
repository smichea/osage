// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
// Measure the world-space lowest *skinned vertex* on each foot of an authored clip.
import {readFile} from "node:fs/promises";
import {dirname, resolve} from "node:path";
import {GLTFLoader} from "three/addons/loaders/GLTFLoader.js";

// Node supplies fetch and Event, but this three.js loader also reports byte counts.
globalThis.ProgressEvent ??= class extends Event {
    constructor(type, init) {
        super(type);
        Object.assign(this, init);
    }
};

export async function loadModel(path) {
    const content = await readFile(path);
    let doc, binary;
    if (content.toString("ascii", 0, 4) === "glTF") {
        const size = content.readUInt32LE(12);
        doc = JSON.parse(content.subarray(20, 20 + size));
        binary = content.subarray(28 + size);
    } else {
        doc = JSON.parse(content);
    }
    // Texture sampling does not participate in linear-blend skinning.
    for (const mesh of doc.meshes ?? []) {
        for (const primitive of mesh.primitives) delete primitive.material;
    }
    for (const field of ["materials", "images", "samplers", "textures"]) delete doc[field];
    for (const buffer of doc.buffers ?? []) {
        if (buffer.uri?.startsWith("data:")) continue;
        const data = buffer.uri ? await readFile(resolve(dirname(path), decodeURIComponent(buffer.uri))) : binary;
        buffer.uri = "data:application/octet-stream;base64," + data.toString("base64");
    }
    return new GLTFLoader().parseAsync(JSON.stringify(doc), "");
}

export function footVertices(scene, root) {
    const group = new Set();
    root.traverse(node => group.add(node));
    const vertices = [];
    scene.traverse(mesh => {
        if (!mesh.isSkinnedMesh) return;
        const {skinIndex, skinWeight} = mesh.geometry.attributes;
        const indices = new Set(mesh.geometry.index?.array ?? Array.from({length: skinIndex.count}, (_, i) => i));
        for (const i of indices) {
            // Count vertices for which the selected subtree owns a strict majority of skin weight.
            let influence = 0;
            for (let j = 0; j < 4; j++) {
                if (group.has(mesh.skeleton.bones[skinIndex.getComponent(i, j)]))
                    influence += skinWeight.getComponent(i, j);
            }
            if (influence > 0.5) vertices.push([mesh, i]);
        }
    });
    return vertices;
}
