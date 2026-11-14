//! Configuración y niveles de optimización para el compilador de bytecode.

use std::fmt;

/// Nivel general de optimización.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum OptLevel {
    /// -O0: Sin optimizaciones (predeterminado).
    #[default]
    O0 = 0,
    /// -O1: Optimizaciones básicas (folding, DCE, simplificación de saltos).
    O1 = 1,
    /// -O2: Optimizaciones avanzadas (peephole, superinstrucciones, invoke, globales indexados).
    O2 = 2,
}

impl OptLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "0" | "O0" | "-O0" => Some(OptLevel::O0),
            "1" | "O1" | "-O1" => Some(OptLevel::O1),
            "2" | "O2" | "-O2" => Some(OptLevel::O2),
            _ => None,
        }
    }
}

/// Identificador de optimizaciones individuales configurables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptPass {
    ConstantFolding,
    DeadCodeElimination,
    JumpSimplification,
    Peephole,
    Superinstructions,
    InlineCaches,
    IndexedGlobals,
}

impl OptPass {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "fold" | "folding" | "constant-folding" => Some(OptPass::ConstantFolding),
            "dce" | "dead-code" => Some(OptPass::DeadCodeElimination),
            "jumps" | "jump-simplification" => Some(OptPass::JumpSimplification),
            "peephole" => Some(OptPass::Peephole),
            "super" | "superinstructions" => Some(OptPass::Superinstructions),
            "invoke" | "inline-caches" => Some(OptPass::InlineCaches),
            "globals" | "indexed-globals" => Some(OptPass::IndexedGlobals),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            OptPass::ConstantFolding => "fold",
            OptPass::DeadCodeElimination => "dce",
            OptPass::JumpSimplification => "jumps",
            OptPass::Peephole => "peephole",
            OptPass::Superinstructions => "super",
            OptPass::InlineCaches => "invoke",
            OptPass::IndexedGlobals => "globals",
        }
    }
}

/// Configuración detallada de optimizaciones del compilador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptConfig {
    pub level: OptLevel,
    pub constant_folding: bool,
    pub dead_code_elimination: bool,
    pub jump_simplification: bool,
    pub peephole: bool,
    pub superinstructions: bool,
    pub inline_caches: bool,
    pub indexed_globals: bool,
}

impl Default for OptConfig {
    fn default() -> Self {
        Self::from_level(OptLevel::O0)
    }
}

impl OptConfig {
    /// Crea una configuración estándar para el nivel dado:
    /// - -O0: todas desactivadas.
    /// - -O1: folding, dce, saltos básicos.
    /// - -O2: O1 + peephole, superinstrucciones, inline caches, globales indexados.
    pub fn from_level(level: OptLevel) -> Self {
        match level {
            OptLevel::O0 => Self {
                level,
                constant_folding: false,
                dead_code_elimination: false,
                jump_simplification: false,
                peephole: false,
                superinstructions: false,
                inline_caches: false,
                indexed_globals: false,
            },
            OptLevel::O1 => Self {
                level,
                constant_folding: true,
                dead_code_elimination: true,
                jump_simplification: true,
                peephole: false,
                superinstructions: false,
                inline_caches: false,
                indexed_globals: false,
            },
            OptLevel::O2 => Self {
                level,
                constant_folding: true,
                dead_code_elimination: true,
                jump_simplification: true,
                peephole: true,
                superinstructions: true,
                inline_caches: true,
                indexed_globals: true,
            },
        }
    }

    /// Habilita o deshabilita un pase individual.
    pub fn set_pass(&mut self, pass: OptPass, enabled: bool) {
        match pass {
            OptPass::ConstantFolding => self.constant_folding = enabled,
            OptPass::DeadCodeElimination => self.dead_code_elimination = enabled,
            OptPass::JumpSimplification => self.jump_simplification = enabled,
            OptPass::Peephole => self.peephole = enabled,
            OptPass::Superinstructions => self.superinstructions = enabled,
            OptPass::InlineCaches => self.inline_caches = enabled,
            OptPass::IndexedGlobals => self.indexed_globals = enabled,
        }
    }

    /// Parsea una lista de flags separados por comas (ej. `--opt=fold,dce` o `--no-opt=fold`).
    pub fn apply_flags(&mut self, flags_str: &str, enable: bool) -> Result<(), String> {
        for part in flags_str.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(pass) = OptPass::parse(trimmed) {
                self.set_pass(pass, enable);
            } else {
                return Err(format!(
                    "Optimización desconocida '{}'. Opciones válidas: fold, dce, jumps, peephole, super, invoke, globals",
                    trimmed
                ));
            }
        }
        Ok(())
    }

    /// Retorna un resumen en texto de las optimizaciones habilitadas.
    pub fn enabled_summary(&self) -> String {
        let mut list = Vec::new();
        if self.constant_folding {
            list.push("fold");
        }
        if self.dead_code_elimination {
            list.push("dce");
        }
        if self.jump_simplification {
            list.push("jumps");
        }
        if self.peephole {
            list.push("peephole");
        }
        if self.superinstructions {
            list.push("super");
        }
        if self.inline_caches {
            list.push("invoke");
        }
        if self.indexed_globals {
            list.push("globals");
        }

        if list.is_empty() {
            "none".to_string()
        } else {
            list.join("+")
        }
    }
}

impl fmt::Display for OptConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} [{}]", self.level, self.enabled_summary())
    }
}

