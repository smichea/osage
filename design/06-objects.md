# 06 — Objects and their common model tree

Date: 2026-11-22 · Status: **in use in the city object profile**, with wider adoption proposed · From: the maintainer, after the Paris world's trials of probes and of the street's meshes round the walker ([research 10](../research/10-incoming-light.md), [05 — regions](05-regions-and-light.md))

The city object profile has one tree for physical, visual and acoustic geometry. The Paris street also has an older height grid and the kart world has its own collision shapes. Breaking an object and handing it between servers remain design work.

The static file profile now adds independently authored acoustic surfaces to the same tree. It connects the three channels to the native city reader and tests conversion on an original slab. [Object file format](08-object-format.md)

## 1. The two geometries

An object has two geometries, for two uses.

| | Physical geometry | Visual geometry |
|---|---|---|
| For | collisions, stacking, resting, breaking | what is seen |
| Made of | simple closed shapes: boxes, capsules, convex volumes | meshes, as fine as wanted |
| Carries | mass, strength, elasticity, friction (slippery or not) | colour, texture, relief, gloss, transparency |
| Computed by | whoever has authority over the physics | each player's device |

They need not look alike: a bollard is a cylinder to collide with and a detailed mesh to look at.

## 2. One tree

An object holds objects: the street holds a building, which holds a window, which holds a pane. **One tree**, each node of which may carry a physical geometry and a visual geometry of its own, coarser than its children's. (Two trees with a table between them were the other way; one tree is what lets a pane that breaks for the physics go from the picture without a table to keep in step.)

The tree is then the levels of detail, on both sides:

- **From afar**, stop high: the building is a box to collide with and a plain façade to see.
- **From near**, go down: each window, each sill.

A node's own geometry stands for all that is under it. Drawing or colliding takes a cut through the tree — for each branch, the node where one stops — and the cut may differ for the eye and for the physics, and from one player to another.

### Identity in the implementation

A node is a placed part. Its parent, children and local transform define its place in the model. Two nodes can use the same mesh. At run time each drawn instance, physical surface and sound binding points back to the owning node. A ray or first contact can therefore return the exact path as a sequence of IDs, including IDs which themselves contain a slash. A visible detail switch preserves the identities of the other channels. A tile or a spatial search tree may split the geometry for fast queries. Each fragment still belongs to its source node. A parent which has no geometry remains in the model and in the city's Bevy entity hierarchy. A parent with an explicitly authored `subtreeProxy` can substitute for its entire branch at a chosen level independently for the eye, contact and sound. [Decision D-149](decisions.md#d-149--keep-the-model-tree-through-runtime-queries) · [File format](08-object-format.md)

## 3. What a coarse node carries

**The aggregate of its children**: their mass together, the colour they average to, the friction of what one would touch. The physics from afar and the picture from afar are then right on average, and going down a level changes the detail, not the sum. A node's aggregates are made when the object is made (or when its children change), not at each use.

## 4. Where it meets the light

- **The meshes round the walker** ([research 10](../research/10-incoming-light.md), the ring): they would be the tree's coarse visual nodes, lit by the probes. Simplifying the street's meshes by merging vertices, at launch, moved the façades' edges and laid the textures wrong (27.7 dB at cells of 0.1 m, 25.4 at 0.5 m, against 28.0): a coarse node's geometry is an object's datum, made once by its author or by a tool that keeps edges and seams.
- **A surface's own colour**, baked beside the probes for that trial, is a property of the visual geometry: the probes then carry the light alone.
- **Between regions** ([05](05-regions-and-light.md)): in the band two regions share, it is the physical geometries the two servers hold together. They are simple: little to keep in step.

## 5. What is not decided

1. **Breaking and bending.** A physical node that gives way is replaced by its children. Either a pane has its shards as children from the start, or they are made at the blow. The first is simple and heavy; the second needs a rule to make them, and both servers of an overlap to make the same.
2. **Things with no inside.** Bistro's façades are shells, not volumes. The physics needs a thickness: invented when the object is made, or asked of whoever models it.
3. **What exists.** Paris's walk grid and the kart's collision shapes are each a physical geometry without a tree; how they become objects, or sit beside them, is to be said.
4. **Who makes the coarse nodes.** By hand, by a tool, or both; and what the tool is held to (silhouette, colour, mass kept).
5. **Identity.** An object handed from a server to another, or broken in two, needs a name that survives it.

## 6. A first step

Without touching the physics: give the Paris street's model a second, coarse visual level for its buildings — made once, by a mesh simplifier that keeps edges and texture seams — and draw the ring round the walker from it. Measured as the ring's other trials were: against Cycles from the scene camera, the triangles drawn, and what the server would have to send.
