use bevy::asset::Handle;
use bevy::reflect::TypeUuid;
use bevy::render::{color::Color, renderer::RenderResources, shader::ShaderDefs, texture::Texture};

/// A material with "standard" properties used in PBR lighting
/// Standard property values with pictures here https://google.github.io/filament/Material%20Properties.pdf
#[derive(Debug, RenderResources, ShaderDefs, TypeUuid)]
#[uuid = "42b444f8-9a77-4e53-9075-863f43f66b9e"]
pub struct ArrayMaterial {
    /// Doubles as diffuse albedo for non-metallic, specular for metallic and a mix for everything in between
    /// If used together with a base_color_texture, this is factored into the final base color
    /// as `base_color * base_color_texture_value`
    pub base_color: Color,
    #[shader_def]
    pub base_color_texture: Option<Handle<Texture>>,
    /// Linear perceptual roughness, clamped to [0.089, 1.0] in the shader
    /// Defaults to minimum of 0.089
    /// If used together with a roughness/metallic texture, this is factored into the final base color
    /// as `roughness * roughness_texture_value`
    pub roughness: f32,
    /// From [0.0, 1.0], dielectric to pure metallic
    /// If used together with a roughness/metallic texture, this is factored into the final base color
    /// as `metallic * metallic_texture_value`
    pub metallic: f32,
    /// Specular intensity for non-metals on a linear scale of [0.0, 1.0]
    /// defaults to 0.5 which is mapped to 4% reflectance in the shader
    pub reflectance: f32,
    #[render_resources(ignore)]
    #[shader_def]
    pub unlit: bool,
}

impl Default for ArrayMaterial {
    fn default() -> Self {
        ArrayMaterial {
            base_color: Color::rgb(1.0, 1.0, 1.0),
            base_color_texture: None,
            // This is the minimum the roughness is clamped to in shader code
            // See https://google.github.io/filament/Filament.html#materialsystem/parameterization/
            // It's the minimum floating point value that won't be rounded down to 0 in the calculations used.
            // Although technically for 32-bit floats, 0.045 could be used.
            roughness: 0.089,
            // Few materials are purely dielectric or metallic
            // This is just a default for mostly-dielectric
            metallic: 0.01,
            // Minimum real-world reflectance is 2%, most materials between 2-5%
            // Expressed in a linear scale and equivalent to 4% reflectance see https://google.github.io/filament/Material%20Properties.pdf
            reflectance: 0.5,
            unlit: false,
        }
    }
}

impl From<Color> for ArrayMaterial {
    fn from(color: Color) -> Self {
        ArrayMaterial {
            base_color: color,
            ..Default::default()
        }
    }
}

impl From<Handle<Texture>> for ArrayMaterial {
    fn from(texture: Handle<Texture>) -> Self {
        ArrayMaterial {
            base_color_texture: Some(texture),
            ..Default::default()
        }
    }
}

/// The layer index into an `ArrayMaterial`.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct MaterialLayer(pub u8);

impl MaterialLayer {
    pub const NULL: Self = Self(std::u8::MAX);
}

pub trait MaterialVoxel {
    fn material(&self) -> MaterialLayer;
}
