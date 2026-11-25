# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# Linux x86_64 release, System V ABI. Jolt Shape::Stats returns size_t first in RAX.
set pagination off
set debuginfod enabled off
set print thread-events off
set breakpoint pending on
python
import collections, gdb, json, os
engine = os.environ['OSAGE_NATIVE_ENGINE']
counts = collections.Counter()
sizes = {}
initial = set()
gdb.execute('set environment LD_PRELOAD ' + os.environ['OSAGE_SHAPE_OBSERVER'])

def value(expression):
    return int(gdb.parse_and_eval(expression))

def record(pointer, byte_count, stage):
    if pointer:
        if stage == 'initial':
            initial.add(pointer)
        if pointer not in sizes:
            sizes[pointer] = byte_count()

def jolt_shape(pointer, stage):
    record(pointer, lambda: value("((unsigned long (*)(void *))'JPH::MeshShape::GetStats() const')((void*)%d)" % pointer), stage)

def rapier_shape(receiver, stage):
    prefix = 'builder' if stage == 'initial' else 'installed'
    call = '(unsigned long)osage_' + prefix + '_trimesh%s((void*)%d)'
    ptr = value(call % ('', receiver))
    record(ptr, lambda: value(call % ('_bytes', receiver)), stage)
    return bool(ptr)

class Installed(gdb.FinishBreakpoint):
    def __init__(self, receiver):
        super().__init__(internal=True)
        self.receiver = receiver
    def stop(self):
        rapier_shape(self.receiver, 'promoted')
        return False

class Hook(gdb.Breakpoint):
    def __init__(self, symbol, event):
        super().__init__(symbol, internal=True)
        self.event = event
    def stop(self):
        if self.event == 'jolt-body':
            if value('$rcx') == 0:
                counts['fixed_colliders'] += 1
                jolt_shape(value('$rdi'), 'initial')
        elif self.event == 'jolt-replace':
            counts['replacements'] += 1
            jolt_shape(value('$rdx'), 'promoted')
        elif self.event == 'rapier-builder':
            if rapier_shape(value('$rsi'), 'initial'):
                counts['fixed_colliders'] += 1
        elif self.event == 'rapier-replace':
            counts['replacements'] += 1
            Installed(value('$rdi'))
        else:
            counts['mesh_constructors'] += 1
        return False

if engine == 'jolt':
    Hook('JPH::MeshShapeSettings::Create() const', 'build')
    Hook('JPH_BodyCreationSettings_Create3', 'jolt-body')
    Hook('JPH_BodyInterface_SetShape', 'jolt-replace')
else:
    Hook('osage_dynamics::mesh_contacts::collider', 'build')
    Hook('<rapier3d::geometry::collider::ColliderBuilder>::build', 'rapier-builder')
    Hook('<rapier3d::geometry::collider::Collider>::set_shape', 'rapier-replace')
gdb.execute('run')
print('OSAGE_SHAPE_MEMORY ' + json.dumps(dict(counts, initial_shapes=len(initial), initial_bytes=sum(sizes[p] for p in initial), total_shapes_seen=len(sizes), unique_shape_bytes_seen=sum(sizes.values()))))
end
