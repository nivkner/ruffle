//! Pixel bender bytecode parsing code.
//! This is heavily based on https://github.com/jamesward/pbjas and https://github.com/HaxeFoundation/format/tree/master/format/pbj

#[cfg(test)]
mod tests;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use downcast_rs::{impl_downcast, Downcast};
use gc_arena::Collect;
use num_traits::FromPrimitive;
use std::{
    fmt::{Debug, Display, Formatter},
    io::Read,
    sync::Arc,
};

use crate::bitmap::BitmapHandle;

/// The name of a special parameter, which gets automatically filled in with the coordinates
/// of the pixel being processed.
pub const OUT_COORD_NAME: &str = "_OutCoord";

#[derive(Clone, Debug, Collect)]
#[collect(require_static)]
pub struct PixelBenderShaderHandle(pub Arc<dyn PixelBenderShaderImpl>);

pub trait PixelBenderShaderImpl: Downcast + Debug {
    fn parsed_shader(&self) -> &PixelBenderShader;
}
impl_downcast!(PixelBenderShaderImpl);

#[repr(u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum PixelBenderType {
    TFloat(f32) = 0x1,
    TFloat2(f32, f32) = 0x2,
    TFloat3(f32, f32, f32) = 0x3,
    TFloat4(f32, f32, f32, f32) = 0x4,
    TFloat2x2([f32; 4]) = 0x5,
    TFloat3x3([f32; 9]) = 0x6,
    TFloat4x4([f32; 16]) = 0x7,
    TInt(i16) = 0x8,
    TInt2(i16, i16) = 0x9,
    TInt3(i16, i16, i16) = 0xA,
    TInt4(i16, i16, i16, i16) = 0xB,
    TString(String) = 0xC,
}

// FIXME - come up with a way to reduce duplication here
#[derive(num_derive::FromPrimitive, Debug, PartialEq, Clone, Copy)]
pub enum PixelBenderTypeOpcode {
    TFloat = 0x1,
    TFloat2 = 0x2,
    TFloat3 = 0x3,
    TFloat4 = 0x4,
    TFloat2x2 = 0x5,
    TFloat3x3 = 0x6,
    TFloat4x4 = 0x7,
    TInt = 0x8,
    TInt2 = 0x9,
    TInt3 = 0xA,
    TInt4 = 0xB,
    TString = 0xC,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum PixelBenderRegChannel {
    R = 0,
    G = 1,
    B = 2,
    A = 3,
}

impl PixelBenderRegChannel {
    pub const RGBA: [PixelBenderRegChannel; 4] = [
        PixelBenderRegChannel::R,
        PixelBenderRegChannel::G,
        PixelBenderRegChannel::B,
        PixelBenderRegChannel::A,
    ];
}

#[derive(Debug, PartialEq, Clone)]
pub struct PixelBenderReg {
    pub index: u32,
    pub channels: Vec<PixelBenderRegChannel>,
    pub kind: PixelBenderRegKind,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PixelBenderRegKind {
    Float,
    Int,
}

#[derive(num_derive::FromPrimitive, Debug, PartialEq, Clone, Copy)]
pub enum PixelBenderParamQualifier {
    Input = 1,
    Output = 2,
}

impl Display for PixelBenderTypeOpcode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PixelBenderTypeOpcode::TFloat => "float",
                PixelBenderTypeOpcode::TFloat2 => "float2",
                PixelBenderTypeOpcode::TFloat3 => "float3",
                PixelBenderTypeOpcode::TFloat4 => "float4",
                PixelBenderTypeOpcode::TFloat2x2 => "matrix2x2",
                PixelBenderTypeOpcode::TFloat3x3 => "matrix3x3",
                PixelBenderTypeOpcode::TFloat4x4 => "matrix4x4",
                PixelBenderTypeOpcode::TInt => "int",
                PixelBenderTypeOpcode::TInt2 => "int2",
                PixelBenderTypeOpcode::TInt3 => "int3",
                PixelBenderTypeOpcode::TInt4 => "int4",
                PixelBenderTypeOpcode::TString => "string",
            }
        )
    }
}

#[derive(num_derive::FromPrimitive, Debug, PartialEq, Clone, Copy)]
pub enum Opcode {
    Nop = 0x0,
    Add = 0x1,
    Sub = 0x2,
    Mul = 0x3,
    Rcp = 0x4,
    Div = 0x5,
    Atan2 = 0x6,
    Pow = 0x7,
    Mod = 0x8,
    Min = 0x9,
    Max = 0xA,
    Step = 0xB,
    Sin = 0xC,
    Cos = 0xD,
    Tan = 0xE,
    Asin = 0xF,
    Acos = 0x10,
    Atan = 0x11,
    Exp = 0x12,
    Exp2 = 0x13,
    Log = 0x14,
    Log2 = 0x15,
    Sqrt = 0x16,
    RSqrt = 0x17,
    Abs = 0x18,
    Sign = 0x19,
    Floor = 0x1A,
    Ceil = 0x1B,
    Fract = 0x1C,
    Mov = 0x1D,
    FloatToInt = 0x1E,
    IntToFloat = 0x1F,
    MatMatMul = 0x20,
    VecMatMul = 0x21,
    MatVecMul = 0x22,
    Normalize = 0x23,
    Length = 0x24,
    Distance = 0x25,
    DotProduct = 0x26,
    CrossProduct = 0x27,
    Equal = 0x28,
    NotEqual = 0x29,
    LessThan = 0x2A,
    LessThanEqual = 0x2B,
    LogicalNot = 0x2C,
    LogicalAnd = 0x2D,
    LogicalOr = 0x2E,
    LogicalXor = 0x2F,
    SampleNearest = 0x30,
    SampleLinear = 0x31,
    LoadIntOrFloat = 0x32,
    Loop = 0x33,
    If = 0x34,
    Else = 0x35,
    EndIf = 0x36,
    FloatToBool = 0x37,
    BoolToFloat = 0x38,
    IntToBool = 0x39,
    BoolToInt = 0x3A,
    VectorEqual = 0x3B,
    VectorNotEqual = 0x3C,
    BoolAny = 0x3D,
    BoolAll = 0x3E,
    PBJMeta1 = 0xA0,
    PBJParam = 0xA1,
    PBJMeta2 = 0xA2,
    PBJParamTexture = 0xA3,
    Name = 0xA4,
    Version = 0xA5,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Operation {
    Nop,
    Normal {
        opcode: Opcode,
        dst: PixelBenderReg,
        src: PixelBenderReg,
    },
    LoadInt {
        dst: PixelBenderReg,
        val: i32,
    },
    LoadFloat {
        dst: PixelBenderReg,
        val: f32,
    },
    If {
        src: PixelBenderReg,
    },
    SampleNearest {
        dst: PixelBenderReg,
        src: PixelBenderReg,
        tf: u8,
    },
    SampleLinear {
        dst: PixelBenderReg,
        src: PixelBenderReg,
        tf: u8,
    },
    Else,
    EndIf,
}

#[derive(Debug, Clone)]
pub enum PixelBenderShaderArgument {
    ImageInput {
        index: u8,
        channels: u8,
        name: String,
        texture: BitmapHandle,
    },
    ValueInput {
        index: u8,
        value: PixelBenderType,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct PixelBenderShader {
    pub name: String,
    pub version: i32,
    pub params: Vec<PixelBenderParam>,
    pub metadata: Vec<PixelBenderMetadata>,
    pub operations: Vec<Operation>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PixelBenderParam {
    Normal {
        qualifier: PixelBenderParamQualifier,
        param_type: PixelBenderTypeOpcode,
        reg: PixelBenderReg,
        name: String,
        metadata: Vec<PixelBenderMetadata>,
    },
    Texture {
        index: u8,
        channels: u8,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PixelBenderMetadata {
    pub key: String,
    pub value: PixelBenderType,
}

/// Parses PixelBender bytecode
pub fn parse_shader(mut data: &[u8]) -> Result<PixelBenderShader, Box<dyn std::error::Error>> {
    let mut shader = PixelBenderShader {
        name: String::new(),
        version: 0,
        params: Vec::new(),
        metadata: Vec::new(),
        operations: Vec::new(),
    };
    let data = &mut data;
    let mut metadata = Vec::new();
    while !data.is_empty() {
        read_op(data, &mut shader, &mut metadata)?;
    }
    // Any metadata left in the vec is associated with our final parameter.
    apply_metadata(&mut shader, &mut metadata);
    Ok(shader)
}

fn read_src_reg(val: u32, size: u8) -> Result<PixelBenderReg, Box<dyn std::error::Error>> {
    const CHANNELS: [PixelBenderRegChannel; 4] = [
        PixelBenderRegChannel::R,
        PixelBenderRegChannel::G,
        PixelBenderRegChannel::B,
        PixelBenderRegChannel::A,
    ];

    let swizzle = val >> 16;
    let mut channels = Vec::new();
    for i in 0..size {
        channels.push(CHANNELS[(swizzle >> (6 - i * 2) & 3) as usize])
    }

    let kind = if val & 0x8000 != 0 {
        PixelBenderRegKind::Int
    } else {
        PixelBenderRegKind::Float
    };

    Ok(PixelBenderReg {
        // Mask off the 0x8000 bit
        index: val & 0x7FFF,
        channels,
        kind,
    })
}

fn read_dst_reg(val: u16, mask: u8) -> Result<PixelBenderReg, Box<dyn std::error::Error>> {
    let mut channels = Vec::new();
    if mask & 0x8 != 0 {
        channels.push(PixelBenderRegChannel::R);
    }
    if mask & 0x4 != 0 {
        channels.push(PixelBenderRegChannel::G);
    }
    if mask & 0x2 != 0 {
        channels.push(PixelBenderRegChannel::B);
    }
    if mask & 0x1 != 0 {
        channels.push(PixelBenderRegChannel::A);
    }

    let kind = if val & 0x8000 != 0 {
        PixelBenderRegKind::Int
    } else {
        PixelBenderRegKind::Float
    };

    Ok(PixelBenderReg {
        // Mask off the 0x8000 bit
        index: (val & 0x7FFF) as u32,
        channels,
        kind,
    })
}

fn read_op<R: Read>(
    data: &mut R,
    shader: &mut PixelBenderShader,
    metadata: &mut Vec<PixelBenderMetadata>,
) -> Result<(), Box<dyn std::error::Error>> {
    let raw = data.read_u8()?;
    let opcode = Opcode::from_u8(raw).expect("Unknown opcode");
    match opcode {
        Opcode::Nop => {
            assert_eq!(data.read_u32::<LittleEndian>()?, 0);
            assert_eq!(data.read_u16::<LittleEndian>()?, 0);
            shader.operations.push(Operation::Nop);
        }
        Opcode::PBJMeta1 | Opcode::PBJMeta2 => {
            let meta_type = data.read_u8()?;
            let meta_key = read_string(data)?;
            let meta_value = read_value(
                data,
                PixelBenderTypeOpcode::from_u8(meta_type)
                    .unwrap_or_else(|| panic!("Unexpected meta type {meta_type}")),
            )?;
            metadata.push(PixelBenderMetadata {
                key: meta_key,
                value: meta_value,
            });
        }
        Opcode::PBJParam => {
            let qualifier = data.read_u8()?;
            let param_type = data.read_u8()?;
            let reg = data.read_u16::<LittleEndian>()?;
            let mask = data.read_u8()?;
            let name = read_string(data)?;

            let param_type = PixelBenderTypeOpcode::from_u8(param_type).unwrap_or_else(|| {
                panic!("Unexpected param type {param_type}");
            });
            let qualifier = PixelBenderParamQualifier::from_u8(qualifier)
                .unwrap_or_else(|| panic!("Unexpected param qualifier {qualifier:?}"));
            apply_metadata(shader, metadata);

            match param_type {
                PixelBenderTypeOpcode::TFloat2x2
                | PixelBenderTypeOpcode::TFloat3x3
                | PixelBenderTypeOpcode::TFloat4x4 => {
                    panic!("Unsupported param type {param_type:?}");
                }
                _ => {}
            }

            let dst_reg = read_dst_reg(reg, mask)?;

            shader.params.push(PixelBenderParam::Normal {
                qualifier,
                param_type,
                reg: dst_reg,
                name,
                metadata: Vec::new(),
            })
        }
        Opcode::PBJParamTexture => {
            let index = data.read_u8()?;
            let channels = data.read_u8()?;
            let name = read_string(data)?;
            apply_metadata(shader, metadata);

            shader.params.push(PixelBenderParam::Texture {
                index,
                channels,
                name,
            });
        }
        Opcode::Name => {
            let len = data.read_u16::<LittleEndian>()?;
            let mut string_bytes = vec![0; len as usize];
            data.read_exact(&mut string_bytes)?;
            shader.name = String::from_utf8(string_bytes)?;
        }
        Opcode::Version => {
            shader.version = data.read_i32::<LittleEndian>()?;
        }
        Opcode::If => {
            assert_eq!(read_uint24(data)?, 0);
            let src = read_uint24(data)?;
            assert_eq!(data.read_u8()?, 0);
            let src_reg = read_src_reg(src, 1)?;
            shader.operations.push(Operation::If { src: src_reg });
        }
        Opcode::Else => {
            assert_eq!(data.read_u32::<LittleEndian>()?, 0);
            assert_eq!(read_uint24(data)?, 0);
            shader.operations.push(Operation::Else);
        }
        Opcode::EndIf => {
            assert_eq!(data.read_u32::<LittleEndian>()?, 0);
            assert_eq!(read_uint24(data)?, 0);
            shader.operations.push(Operation::EndIf);
        }
        Opcode::LoadIntOrFloat => {
            let dst = data.read_u16::<LittleEndian>()?;
            let mask = data.read_u8()?;
            assert_eq!(mask & 0xF, 0);
            let dst_reg = read_dst_reg(dst, mask >> 4)?;
            match dst_reg.kind {
                PixelBenderRegKind::Float => {
                    let val = read_float(data)?;
                    shader
                        .operations
                        .push(Operation::LoadFloat { dst: dst_reg, val })
                }
                PixelBenderRegKind::Int => {
                    let val = data.read_i32::<LittleEndian>()?;
                    shader
                        .operations
                        .push(Operation::LoadInt { dst: dst_reg, val })
                }
            }
        }
        Opcode::SampleNearest | Opcode::SampleLinear => {
            let dst = data.read_u16::<LittleEndian>()?;
            let mask = data.read_u8()?;
            let src = read_uint24(data)?;
            let tf = data.read_u8()?;

            let dst_reg = read_dst_reg(dst, mask >> 4)?;
            let src_reg = read_src_reg(src, 2)?;

            match opcode {
                Opcode::SampleNearest => shader.operations.push(Operation::SampleNearest {
                    dst: dst_reg,
                    src: src_reg,
                    tf,
                }),
                Opcode::SampleLinear => shader.operations.push(Operation::SampleLinear {
                    dst: dst_reg,
                    src: src_reg,
                    tf,
                }),
                _ => unreachable!(),
            }
        }
        _ => {
            let dst = data.read_u16::<LittleEndian>()?;
            let mut mask = data.read_u8()?;
            let size = (mask & 0x3) + 1;
            let matrix = (mask >> 2) & 3;
            let src = read_uint24(data)?;
            assert_eq!(data.read_u8()?, 0, "Unexpected u8 for opcode {opcode:?}");
            mask >>= 4;

            let src_reg = read_src_reg(src, size)?;
            let dst_reg = if matrix != 0 {
                assert_eq!(src >> 16, 0);
                assert_eq!(size, 1);
                panic!("Matrix with mask {mask:b} matrix {matrix:b}");
            } else {
                read_dst_reg(dst, mask)?
            };
            shader.operations.push(Operation::Normal {
                opcode,
                dst: dst_reg,
                src: src_reg,
            })
        }
    };
    Ok(())
}

fn read_string<R: Read>(data: &mut R) -> Result<String, Box<dyn std::error::Error>> {
    let mut string = String::new();
    let mut b = data.read_u8()?;
    while b != 0 {
        string.push(b as char);
        b = data.read_u8()?;
    }
    Ok(string)
}

fn read_float<R: Read>(data: &mut R) -> Result<f32, Box<dyn std::error::Error>> {
    Ok(data.read_f32::<BigEndian>()?)
}

fn read_value<R: Read>(
    data: &mut R,
    opcode: PixelBenderTypeOpcode,
) -> Result<PixelBenderType, Box<dyn std::error::Error>> {
    match opcode {
        PixelBenderTypeOpcode::TFloat => Ok(PixelBenderType::TFloat(read_float(data)?)),
        PixelBenderTypeOpcode::TFloat2 => Ok(PixelBenderType::TFloat2(
            read_float(data)?,
            read_float(data)?,
        )),
        PixelBenderTypeOpcode::TFloat3 => Ok(PixelBenderType::TFloat3(
            read_float(data)?,
            read_float(data)?,
            read_float(data)?,
        )),
        PixelBenderTypeOpcode::TFloat4 => Ok(PixelBenderType::TFloat4(
            read_float(data)?,
            read_float(data)?,
            read_float(data)?,
            read_float(data)?,
        )),
        PixelBenderTypeOpcode::TFloat2x2 => Ok(PixelBenderType::TFloat2x2([
            read_float(data)?,
            read_float(data)?,
            read_float(data)?,
            read_float(data)?,
        ])),
        PixelBenderTypeOpcode::TFloat3x3 => {
            let mut floats: [f32; 9] = [0.0; 9];
            for float in &mut floats {
                *float = read_float(data)?;
            }
            Ok(PixelBenderType::TFloat3x3(floats))
        }
        PixelBenderTypeOpcode::TFloat4x4 => {
            let mut floats: [f32; 16] = [0.0; 16];
            for float in &mut floats {
                *float = read_float(data)?;
            }
            Ok(PixelBenderType::TFloat4x4(floats))
        }
        PixelBenderTypeOpcode::TInt => Ok(PixelBenderType::TInt(data.read_i16::<LittleEndian>()?)),
        PixelBenderTypeOpcode::TInt2 => Ok(PixelBenderType::TInt2(
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
        )),
        PixelBenderTypeOpcode::TInt3 => Ok(PixelBenderType::TInt3(
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
        )),
        PixelBenderTypeOpcode::TInt4 => Ok(PixelBenderType::TInt4(
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
            data.read_i16::<LittleEndian>()?,
        )),
        PixelBenderTypeOpcode::TString => Ok(PixelBenderType::TString(read_string(data)?)),
    }
}

fn read_uint24<R: Read>(data: &mut R) -> Result<u32, Box<dyn std::error::Error>> {
    let ch1 = data.read_u8()? as u32;
    let ch2 = data.read_u8()? as u32;
    let ch3 = data.read_u8()? as u32;
    Ok(ch1 | (ch2 << 8) | (ch3 << 16))
}

// The opcodes are laid out like this:
//
// ```
// PBJMeta1 (for overall program)
// PBJMeta1 (for overall program)
// PBJParam (param 1)
// ...
// PBJMeta1 (for param 1)
// PBJMeta1 (for param 1)
// ...
// PBJParam (param 2)
// ,,,
// PBJMeta2 (for param 2)
// ```
//
// The metadata associated with parameter is determined by all of the metadata opcodes
// that come after it and before the next parameter opcode. The metadata opcodes
// that come before all params are associated with the overall program.

fn apply_metadata(shader: &mut PixelBenderShader, metadata: &mut Vec<PixelBenderMetadata>) {
    // Reset the accumulated metadata Vec - we will start accumulating metadata for the next param
    let metadata = std::mem::take(metadata);
    match shader.params.last_mut() {
        Some(PixelBenderParam::Normal { metadata: meta, .. }) => {
            *meta = metadata;
        }
        Some(param) => {
            if !metadata.is_empty() {
                panic!("Tried to apply metadata to texture parameter {param:?}")
            }
        }
        None => {
            shader.metadata = metadata;
        }
    }
}
