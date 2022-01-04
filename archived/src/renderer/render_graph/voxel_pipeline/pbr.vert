#version 450

layout(location = 0) in vec3 Vertex_Position;
layout(location = 1) in vec3 Vertex_Normal;
layout(location = 2) in uint Vertex_MaterialWeights;

layout(location = 0) out vec3 v_WorldPosition;
layout(location = 1) out vec3 v_WorldNormal;
layout(location = 2) out vec4 v_MaterialWeights;

layout(set = 0, binding = 0) uniform CameraViewProj {
    mat4 ViewProj;
};

layout(set = 2, binding = 0) uniform Transform {
    mat4 Model;
};

void main() {
    vec4 world_position = Model * vec4(Vertex_Position, 1.0);

    // Each byte is the count of voxels of a given material adjacent to the vertex.
    v_MaterialWeights = vec4(
        Vertex_MaterialWeights & 0xFF,
        Vertex_MaterialWeights >> 8 & 0xFF,
        Vertex_MaterialWeights >> 16 & 0xFF,
        Vertex_MaterialWeights >> 24 & 0xFF
    );
    v_MaterialWeights /= (v_MaterialWeights.x + v_MaterialWeights.y + v_MaterialWeights.z + v_MaterialWeights.w);
    v_WorldPosition = world_position.xyz;
    v_WorldNormal = mat3(Model) * Vertex_Normal;
    gl_Position = ViewProj * world_position;
}
